-- 0007_account_boards: the account's board list (the "your boards follow
-- you" roadmap, M2; plan lp2025/2026-10-19-2305-account-board-list).
--
-- One row per board on one account's list, so a board that ever joined
-- shows on every computer the account signs in on, offline included. A row
-- is written when a board registers with the relay proving the account's
-- key, or when Studio finds the account's current key on a board it is
-- linked to; only the account's Forget removes it. A row grants nothing:
-- the relay's own proof still gates presence, pictures and routes.
--
-- New table, nothing to backfill: lists fill as boards next reach the relay
-- or a Studio.
--
-- board_id is the board's MAC as twelve lowercase hex digits (what
-- /relay/board/<id> takes). Times are REAL f64 epoch seconds, like every
-- other time here. model is NULL until a Studio reports one (the relay's
-- hello carries none). The four seen_*_at columns are when each kind of
-- link last saw the board.
--
-- The last picture lives in the row, written behind by the relay (when the
-- board leaves, on a graceful shutdown, and at most every ten minutes while
-- it is online), never per picture. Its four columns are all set or all
-- NULL, and they live here rather than in a child table because they are
-- only ever read and written whole, with the row:
--   picture_outputs  lamps per output, each a u32 little-endian, concatenated
--   picture_colors   R, G, B per sample
--   picture_accounts the account uids that may read it (those the board
--                    proved when the picture was taken), joined by ','
--                    (a uid is base-32 and never holds one)

CREATE TABLE account_boards (
    user_uid         TEXT NOT NULL REFERENCES users (uid) ON DELETE CASCADE,
    board_id         TEXT NOT NULL,
    label            TEXT NOT NULL,
    model            TEXT,
    firmware         TEXT,
    project          TEXT,
    joined_at        REAL NOT NULL,
    seen_relay_at    REAL,
    seen_usb_at      REAL,
    seen_ble_at      REAL,
    seen_lan_at      REAL,
    picture_at       REAL,
    picture_outputs  BLOB,
    picture_colors   BLOB,
    picture_accounts TEXT,
    PRIMARY KEY (user_uid, board_id)
);

-- The relay writes per board: every account's row of the board that came
-- or went.
CREATE INDEX account_boards_by_board ON account_boards (board_id);
