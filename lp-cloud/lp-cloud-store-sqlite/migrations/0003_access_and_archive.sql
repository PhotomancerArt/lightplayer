-- 0003_access_and_archive: the access model replaces the two-value
-- visibility, and a project can be archived.
--
-- `projects.visibility` becomes `projects.access`, carrying the API v3
-- vocabulary (`none` | `view` | `edit`) instead of v2's (`private` | `link`).
-- The two old values map onto the bottom two new ones exactly — `private`
-- granted nothing to a link-holder and `link` granted reading — so this is a
-- rename of the column and of its values, not a policy change applied to
-- existing rows. Nothing in the old schema could express `edit`, so no row
-- becomes writable by a stranger as a result of this migration.
--
-- `archived_at` is NULL for every existing row: nothing was archivable
-- before, so nothing is archived.
--
-- `members.role` renames `member` to `editor` for the same reason — the role
-- always meant "reads and writes, but is not the owner", and `editor` says
-- so. `owner` is unchanged.
--
-- ALTER TABLE ... RENAME COLUMN is SQLite 3.25+ (2018); the bundled
-- rusqlite is far past it.

ALTER TABLE projects RENAME COLUMN visibility TO access;

UPDATE projects SET access = CASE access
    WHEN 'private' THEN 'none'
    WHEN 'link'    THEN 'view'
    ELSE access
END;

ALTER TABLE projects ADD COLUMN archived_at REAL;

UPDATE members SET role = 'editor' WHERE role = 'member';
