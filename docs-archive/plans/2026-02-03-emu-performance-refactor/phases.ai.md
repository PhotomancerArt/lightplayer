# Implementation Phases

1. Add FuelExhausted variant to StepResult
2. Refactor step() to use step_inner() (extract fuel-check-free version)
3. Implement run_inner() with tight loop and inline fuel checking
4. Implement run() and run_fuel() public API
5. Reimplement run_until_*() functions using run()
6. Remove max_instructions field and related methods
7. Update call sites to use new API where beneficial
8. Cleanup, review, and validation
