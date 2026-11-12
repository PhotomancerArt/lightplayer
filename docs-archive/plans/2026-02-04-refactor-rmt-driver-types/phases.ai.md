# Refactor RMT Driver to Use LedChannel and LedTransaction Types - Phases

1. **Add ChannelState struct and migrate interrupt handler** - Consolidate global state into `ChannelState` array, update interrupt handler to use it
2. **Add LedChannel type and update test to use it** - Create `LedChannel` struct, update test to create and store it
3. **Add LedChannel::start_transmission() and update test** - Add method to start transmission, update test to use it
4. **Add LedTransaction::wait_complete() and update test** - Complete the transaction pattern, update test to use full new API
5. **Remove legacy wrapper functions** - Remove old API functions, clean up static storage
6. **Cleanup and validation** - Remove debug prints, fix warnings, final validation

Each phase exercises the new code immediately and can be tested independently.
