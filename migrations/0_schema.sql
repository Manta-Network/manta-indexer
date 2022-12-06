-- create shard table
CREATE TABLE IF NOT EXISTS "shards"
(
    shard_index        INTEGER NOT NULL,
    utxo_index         INTEGER NOT NULL,
    utxo               BLOB    NOT NULL,
    full_incoming_note BLOB    NOT NULL
);

-- create index on shards
CREATE INDEX IF NOT EXISTS "shard_index" ON "shards"
(
    shard_index ASC,
    utxo_index
);

-- create nullifier table
CREATE TABLE IF NOT EXISTS "nullifier"
(
    idx                  INTEGER PRIMARY KEY NOT NULL,
    nullifier_commitment BLOB NOT NULL,
    outgoing_note        BLOB NOT NULL
);

UPDATE SQLITE_SEQUENCE SET seq = 0 WHERE name = 'nullifier';

-- create the total of senders and receivers table
CREATE TABLE IF NOT EXISTS "senders_receivers_total"
(
    total INTEGER NOT NULL
);
