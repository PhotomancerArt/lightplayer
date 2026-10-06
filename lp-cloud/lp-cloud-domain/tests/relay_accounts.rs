//! The relay's view of the domain: which accounts a board's registration
//! proves, and whose boards `ListBoards` shows.

use lp_cloud_domain::{CloudService, verify_board_accounts};
use lp_cloud_store_mem::{MemClock, MemIdMint, MemMetaStore};
use lpc_cloud_api::{AccountAccessInfo, Actor, BoardList, CloudError, CloudRequest, CloudResponse};
use lpc_history::PrefixedUid;
use lpc_relay::{relay_auth_key, relay_proof};

type Service = CloudService<MemMetaStore, MemClock, MemIdMint>;

const MAC: [u8; 6] = [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30];
const NONCE: [u8; 32] = [0x42; 32];

#[test]
fn a_proof_made_with_the_installed_account_key_verifies() {
    let mut svc = service();
    let alice = signed_in(&mut svc, "g-alice");
    let bob = signed_in(&mut svc, "g-bob");
    let alices = keys(&mut svc, alice);
    let bobs = keys(&mut svc, bob);

    let salts = [alices.key_salt, [0xee; 16], bobs.key_salt];
    let proofs = [proof_for(&alices), [0u8; 32], proof_for(&bobs)];
    let verdict = verify_board_accounts(svc.store(), &MAC, &NONCE, &salts, &proofs);
    assert_eq!(verdict.users, [uid(alice), uid(bob)]);
    assert_eq!(verdict.accounts_ok, 0b101, "the unknown salt in the middle");
}

#[test]
fn a_wrong_board_the_raw_secret_or_a_retired_key_proves_nothing() {
    let mut svc = service();
    let alice = signed_in(&mut svc, "g-alice");
    let before = keys(&mut svc, alice);
    let salts = [before.key_salt];

    let wrong_board = relay_proof(&relay_auth_key(&device_key(&before)), &NONCE, &[9; 6]);
    let verdict = verify_board_accounts(svc.store(), &MAC, &NONCE, &salts, &[wrong_board]);
    assert!(verdict.is_empty(), "a proof is bound to its board");

    let raw_secret = relay_proof(&relay_auth_key(&before.key_secret), &NONCE, &MAC);
    let verdict = verify_board_accounts(svc.store(), &MAC, &NONCE, &salts, &[raw_secret]);
    assert!(
        verdict.is_empty(),
        "K is the installed key, not the raw secret"
    );

    let old_proof = proof_for(&before);
    svc.handle(alice, CloudRequest::ResetAccountKey).unwrap();
    let verdict = verify_board_accounts(svc.store(), &MAC, &NONCE, &salts, &[old_proof]);
    assert!(
        verdict.is_empty(),
        "a reset key no longer speaks for the account"
    );
}

#[test]
fn the_same_account_twice_is_one_user_and_a_missing_proof_does_not_verify() {
    let mut svc = service();
    let alice = signed_in(&mut svc, "g-alice");
    let alices = keys(&mut svc, alice);
    let proof = proof_for(&alices);
    let salts = [alices.key_salt; 3];
    let verdict = verify_board_accounts(svc.store(), &MAC, &NONCE, &salts, &[proof, proof]);
    assert_eq!(verdict.users, [uid(alice)]);
    assert_eq!(
        verdict.accounts_ok, 0b011,
        "the third salt carried no proof"
    );
}

#[test]
fn list_boards_shows_an_account_its_own_a_guest_none_and_refuses_anonymous() {
    let mut svc = service();
    let alice = signed_in(&mut svc, "g-alice");
    let guest = Actor::User(svc.begin_guest_user().uid);

    assert_eq!(svc.board_list_viewer(alice), Ok(Some(uid(alice))));
    assert_eq!(svc.board_list_viewer(guest), Ok(None));
    assert_eq!(
        svc.board_list_viewer(Actor::Anonymous),
        Err(CloudError::NotAuthenticated)
    );
    assert_eq!(
        svc.handle(alice, CloudRequest::ListBoards),
        Ok(CloudResponse::BoardList(BoardList::default())),
        "the domain alone knows no presence"
    );
    assert_eq!(
        svc.handle(Actor::Anonymous, CloudRequest::ListBoards),
        Err(CloudError::NotAuthenticated)
    );
}

fn service() -> Service {
    CloudService::new(
        MemMetaStore::new(),
        MemClock::new(1_700_000_000.0),
        MemIdMint::new(),
    )
}

fn signed_in(svc: &mut Service, google_sub: &str) -> Actor {
    let email = format!("{google_sub}@example.com");
    let user = svc.upsert_user(google_sub, &email, "Someone", "google", None, None, None);
    Actor::User(user.uid)
}

fn uid(actor: Actor) -> PrefixedUid {
    match actor {
        Actor::User(uid) => uid,
        Actor::Anonymous => panic!("anonymous"),
    }
}

fn keys(svc: &mut Service, actor: Actor) -> AccountAccessInfo {
    match svc.handle(actor, CloudRequest::GetAccountAccess).unwrap() {
        CloudResponse::AccountAccessInfo(info) => info,
        other => panic!("{other:?}"),
    }
}

/// `K` as Studio installs it on a board.
fn device_key(keys: &AccountAccessInfo) -> [u8; 32] {
    lpc_access::derive_login_key(&keys.key_secret, &keys.key_salt, 1)
}

fn proof_for(keys: &AccountAccessInfo) -> [u8; 32] {
    relay_proof(&relay_auth_key(&device_key(keys)), &NONCE, &MAC)
}
