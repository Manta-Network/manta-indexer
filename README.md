# manta-indexer

## Relaying part

use jsonrpsee framework, and it's organized by method name.
we will rewrite all rpc api that subx contains.

*[State API](https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/mod.rs)*

+ [x] state_call
+ [x] ~~state_getKeys(duplicated)~~
+ [x] state_getPairs
+ [x] state_getKeysPaged
+ [x] state_getStorage
+ [x] state_getStorageHash
+ [x] state_getStorageSize
+ [x] state_getMetadata
+ [x] state_getRuntimeVersion
+ [x] state_queryStorage
+ [x] state_queryStorageAt
+ [ ] state_getReadProof(block by upstream compiling)
+ [x] state_subscribeStorage
+ [x] state_subscribeRuntimeVersion
+ [x] state_traceBlock

*[System API](https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/system/mod.rs#L33)*

+ [x] system_name
+ [x] system_version
+ [x] system_chain
+ [ ] system_chainType(block by upstream compiling)
+ [ ] system_properties(block by upstream compiling)
+ [x] system_health
+ [x] system_localPeerId
+ [x] system_localListenAddresses
+ [ ] system_peers(block by upstream compiling)
+ [ ] system_unstable_networkState
+ [x] system_addReservedPeer
+ [x] system_removeReservedPeer
+ [x] system_reservedPeers
+ [ ] system_nodeRoles(block by upstream compiling)
+ [ ] system_syncState(block by upstream compiling)
+ [x] system_addLogFilter
+ [x] system_resetLogFilter

*[Chain API](https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/chain/mod.rs#L27)*

+ [x] chain_getHeader
+ [ ] chain_getBlock(block by upstream compiling)
+ [x] chain_getBlockHash
+ [x] chain_getFinalizedHead
+ [x] chain_subscribeAllHeads
+ [x] chain_subscribeNewHeads
+ [x] chain_subscribeFinalizedHeads

*[Author API](https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/author/mod.rs#L30)*

+ [x] author_submitExtrinsic
+ [x] author_insertKey
+ [x] author_rotateKeys
+ [x] author_hasSessionKeys
+ [x] author_hasKey
+ [x] author_pendingExtrinsics
+ [ ] author_removeExtrinsic(block by upstream compiling)
+ [ ] author_submitAndWatchExtrinsic(block by upstream compiling)
