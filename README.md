# manta-indexer


## Relaying part

use jsonrpsee framework, and it's organized by method name.
we will rewrite all rpc api that subx contains.

*[State API](https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs)*

+ [x] state_call
+ [x] ~~state_getKeys(duplicated)~~
+ [x] state_getPairs
+ [x] state_getKeysPaged
+ [ ] state_getStorage
+ [ ] state_getStorageHash
+ [ ] state_getStorageSize
+ [x] state_getMetadata
+ [ ] state_getRuntimeVersion
+ [ ] state_queryStorage
+ [x] state_queryStorageAt
+ [ ] state_getReadProof
+ [ ] state_subscribeStorage
+ [ ] state_traceBlock

*[System API](https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L33)*

+ [ ] system_name
+ [ ] system_version
+ [ ] system_chain
+ [ ] system_chainType
+ [ ] system_properties
+ [ ] system_health
+ [ ] system_localPeerId
+ [ ] system_localListenAddresses
+ [ ] system_peers
+ [ ] system_unstable_networkState
+ [ ] system_addReservedPeer
+ [ ] system_reservedPeers
+ [ ] system_nodeRoles
+ [ ] system_syncState
+ [ ] system_addLogFilter
+ [ ] system_resetLogFilter

*[Chain API](https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/chain/mod.rs#L27)*

+ [ ] chain_getHeader
+ [ ] chain_getBlock
+ [ ] chain_getBlockHash
+ [ ] chain_getFinalizedHead
+ [ ] chain_subscribeAllHeads
+ [ ] chain_subscribeNewHeads
+ [ ] chain_subscribeFinalizedHeads


*[Author API](https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/author/mod.rs#L30)*

+ [ ] author_submitExtrinsic
+ [ ] author_insertKey
+ [ ] author_rotateKeys
+ [ ] author_hasSessionKeys
+ [ ] author_hasKey
+ [ ] author_pendingExtrinsics
+ [ ] author_removeExtrinsic
+ [ ] author_submitAndWatchExtrinsic