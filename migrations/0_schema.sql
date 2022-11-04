-- create shard table
CREATE TABLE IF NOT EXISTS "shards"
(
    shard_index INTEGER NOT NULL,
    next_index	INTEGER NOT NULL,
    utxo		BLOB    NOT NULL
);

-- create index on shards
CREATE INDEX IF NOT EXISTS "shard_index" ON "shards" 
(
    shard_index	ASC,
    next_index
);

-- create void number table
CREATE TABLE IF NOT EXISTS "void_number" 
(
    idx         INTEGER PRIMARY KEY NOT NULL,
    vn          BLOB NOT NULL
);

-- create void number table
CREATE TABLE IF NOT EXISTS "senders_receivers_total" 
(
    total	    INTEGER NOT NULL
);
