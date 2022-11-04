# Integration Testing

## How to test
building service.

```
yarn
```

This integration test is driven by command arguments.

The example below means connect local 9988 port as backend, and test_case means
what kind of testing do you want to run.
```
yarn start --ws=ws://127.0.0.1:9988 --test_case=connection
```

test_case list:
 - connection. Just testing the connection with backend, when finishes, just exit.
 - sub_new_heads. Testing the sub rpc: chain_subscribeNewHeads. Normally we can use this to test whether the basic
   subscription works.