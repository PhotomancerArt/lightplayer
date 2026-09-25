-- 0005_account_access: an account's device key and optional account
-- passwords (the "Bluetooth access, easy by default" plan, D7).
--
-- One row per account, minted lazily on the account's first
-- GetAccountAccess, so existing accounts simply have no row yet — nothing
-- to backfill.
--
-- The byte columns are BLOBs of exactly the domain's sizes: key_secret 32,
-- the three salts 16 each. previous_key_salts is the retired account-key
-- salts concatenated oldest first (a multiple of 16 bytes, at most 4 salts);
-- one column rather than a child table because it is only ever read and
-- written whole, with the rest of the row.
--
-- The two passwords are stored readable, by decision: they are shareable
-- device passwords (like a Wi-Fi password) that Settings shows back to the
-- account holder, not credentials for the account itself.

CREATE TABLE account_access (
    user_uid           TEXT PRIMARY KEY NOT NULL REFERENCES users (uid) ON DELETE CASCADE,
    key_secret         BLOB NOT NULL,
    key_salt           BLOB NOT NULL,
    play_password_salt BLOB NOT NULL,
    edit_password_salt BLOB NOT NULL,
    play_password      TEXT,
    edit_password      TEXT,
    previous_key_salts BLOB NOT NULL,
    updated_at         REAL NOT NULL
);
