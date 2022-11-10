import sqlite3

conn = sqlite3.connect("local_sync.db")

c = conn.cursor()
cursor = c.execute("select * from shards order by shard_index, next_index")
origin_shards = {}
origin_vn = []
for row in cursor:
    shard_index = int(row[0])
    next_index = int(row[1])
    data = row[2]
    if shard_index not in origin_shards:
        origin_shards[shard_index] = []
    origin_shards[shard_index].append((next_index, data))

cursor = c.execute("select * from void_number")
for row in cursor:
    idx = int(row[0])
    vn = row[1]
    origin_vn.append((idx, vn))


def expand_times(v, times):
    total = len(v)
    append = []
    for i in range(times):
        new = [(item[0] + i * total, item[1]) for item in v]
        append += new
    return append


c.execute("DELETE FROM shards")
conn.commit()

expand = 100
for (k, v) in origin_shards.items():
    v = expand_times(v, expand)
    for item in v:
        c.execute(f"INSERT INTO shards (shard_index, next_index, utxo) VALUES (?, ?, ?)",
                  (k, item[0], sqlite3.Binary(item[1])))
conn.commit()

c.execute("DELETE FROM void_number")
conn.commit()

expand = 100
vn = expand_times(origin_vn, expand)
for item in vn:
    c.execute(f"INSERT INTO void_number (idx, vn) VALUES (?, ?)", (item[0], sqlite3.Binary(item[1])))
conn.commit()

conn.close()
