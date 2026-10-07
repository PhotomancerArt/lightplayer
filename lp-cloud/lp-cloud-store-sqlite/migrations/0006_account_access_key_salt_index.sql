-- 0006_account_access_key_salt_index: find an account by its key's salt
-- (the cloud relay, Wi-Fi roadmap M7).
--
-- A board registering with the relay names each account key it holds by
-- its salt and proves it holds the key; the hub looks the account up by
-- that salt. Without an index that is a scan of account_access on every
-- registration, and a deploy reconnects every board at once.
--
-- Additive: an index over the column as 0005 stores it (a 16-byte BLOB).
-- Not UNIQUE on purpose: salts are random, so a duplicate cannot happen in
-- practice, but a UNIQUE index would make this migration — and so the
-- deploy that carries it — fail on a database that somehow held one. The
-- lookup answers the lowest user_uid if it ever does.
--
-- Only the current key_salt is indexed. A retired salt (in
-- previous_key_salts) deliberately finds nothing: a reset key no longer
-- speaks for its account.

CREATE INDEX account_access_key_salt ON account_access (key_salt);
