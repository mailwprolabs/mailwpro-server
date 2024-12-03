/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs Ltd <hello@stalw.art>
 *
 * SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-SEL
 */

use ahash::AHashSet;
use common::Server;
use directory::{backend::internal::manage::ManageDirectory, QueryBy};
use imap_proto::ResponseType;
use jmap::{
    email::delete::EmailDeletion,
    mailbox::{INBOX_ID, JUNK_ID, TRASH_ID},
    JmapMethods,
};
use jmap_proto::types::{collection::Collection, id::Id, property::Property};
use store::{
    write::{key::DeserializeBigEndian, TagValue},
    IterateParams, LogKey, U32_LEN, U64_LEN,
};

use crate::{
    directory::internal::TestInternalDirectory,
    imap::{AssertResult, ImapConnection, Type},
    jmap::assert_is_empty,
};

use super::JMAPTest;

pub async fn test(params: &mut JMAPTest) {
    println!("Running purge tests...");
    let server = params.server.clone();
    let client = &mut params.client;
    let inbox_id = Id::from(INBOX_ID).to_string();
    let trash_id = Id::from(TRASH_ID).to_string();
    let junk_id = Id::from(JUNK_ID).to_string();

    // Connect to IMAP
    let account_id = server
        .core
        .storage
        .data
        .create_test_user(
            "jdoe@example.com",
            "12345",
            "John Doe",
            &["jdoe@example.com"],
        )
        .await;

    let mut imap = ImapConnection::connect(b"_x ").await;
    imap.assert_read(Type::Untagged, ResponseType::Ok).await;
    imap.send("LOGIN \"jdoe@example.com\" \"12345\"").await;
    imap.assert_read(Type::Tagged, ResponseType::Ok).await;
    imap.send("STATUS INBOX (UIDNEXT MESSAGES UNSEEN)").await;
    imap.assert_read(Type::Tagged, ResponseType::Ok)
        .await
        .assert_contains("MESSAGES 0");

    // Create test messages
    client.set_default_account_id(Id::from(account_id));
    let mut message_ids = Vec::new();
    let mut pass = 0;
    let mut changes = AHashSet::new();

    loop {
        pass += 1;
        for folder_id in [&inbox_id, &trash_id, &junk_id] {
            message_ids.push(
                client
                    .email_import(
                        format!(
                            concat!(
                                "From: bill@example.com\r\n",
                                "To: jdoe@example.com\r\n",
                                "Subject: TPS Report #{} {}\r\n",
                                "\r\n",
                                "I'm going to need those TPS reports ASAP. ",
                                "So, if you could do that, that'd be great."
                            ),
                            pass, folder_id
                        )
                        .into_bytes(),
                        [folder_id],
                        None::<Vec<&str>>,
                        None,
                    )
                    .await
                    .unwrap()
                    .take_id(),
            );
        }

        if pass == 1 {
            changes = get_changes(&server).await;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        } else {
            break;
        }
    }

    // Check IMAP status
    imap.send("LIST \"\" \"*\" RETURN (STATUS (MESSAGES))")
        .await;
    imap.assert_read(Type::Tagged, ResponseType::Ok)
        .await
        .assert_contains("\"INBOX\" (MESSAGES 2)")
        .assert_contains("\"Deleted Items\" (MESSAGES 2)")
        .assert_contains("\"Junk Mail\" (MESSAGES 2)");

    // Make sure both messages and changes are present
    assert_eq!(
        server
            .get_document_ids(account_id, Collection::Email)
            .await
            .unwrap()
            .unwrap()
            .len(),
        6
    );

    // Purge junk/trash messages and old changes
    server.purge_account(account_id).await;

    // Only 4 messages should remain
    assert_eq!(
        server
            .get_document_ids(account_id, Collection::Email)
            .await
            .unwrap()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        server
            .get_tag(
                account_id,
                Collection::Email,
                Property::MailboxIds,
                TagValue::Id(INBOX_ID)
            )
            .await
            .unwrap()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        server
            .get_tag(
                account_id,
                Collection::Email,
                Property::MailboxIds,
                TagValue::Id(TRASH_ID)
            )
            .await
            .unwrap()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        server
            .get_tag(
                account_id,
                Collection::Email,
                Property::MailboxIds,
                TagValue::Id(JUNK_ID)
            )
            .await
            .unwrap()
            .unwrap()
            .len(),
        1
    );

    // Check IMAP status
    imap.send("LIST \"\" \"*\" RETURN (STATUS (MESSAGES))")
        .await;
    imap.assert_read(Type::Tagged, ResponseType::Ok)
        .await
        .assert_contains("\"INBOX\" (MESSAGES 2)")
        .assert_contains("\"Deleted Items\" (MESSAGES 1)")
        .assert_contains("\"Junk Mail\" (MESSAGES 1)");

    // Compare changes
    let new_changes = get_changes(&server).await;
    assert!(!changes.is_empty());
    assert!(!new_changes.is_empty());
    for change in changes {
        assert!(
            !new_changes.contains(&change),
            "Change {change:?} was not purged"
        );
    }

    // Delete account
    server
        .core
        .storage
        .data
        .delete_principal(QueryBy::Id(account_id))
        .await
        .unwrap();
    assert_is_empty(server).await;
}

async fn get_changes(server: &Server) -> AHashSet<(u64, u8)> {
    let mut changes = AHashSet::new();
    server
        .core
        .storage
        .data
        .iterate(
            IterateParams::new(
                LogKey {
                    account_id: 0,
                    collection: 0,
                    change_id: 0,
                },
                LogKey {
                    account_id: u32::MAX,
                    collection: u8::MAX,
                    change_id: u64::MAX,
                },
            )
            .ascending()
            .no_values(),
            |key, _| {
                changes.insert((
                    key.deserialize_be_u64(key.len() - U64_LEN).unwrap(),
                    key[U32_LEN],
                ));
                Ok(true)
            },
        )
        .await
        .unwrap();
    changes
}
