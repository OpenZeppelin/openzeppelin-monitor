# Fuzz testing with cargo-fuzz (Experimental)

## Setup

Follow the setup instructions in the [cargo-fuzz documentation](https://rust-fuzz.github.io/book/cargo-fuzz/setup.html)

## Running Fuzz Tests

To run the fuzz tests, use the following command:

* for the `expression_parser` target:

  ```bash
  cargo +nightly fuzz run expression_parser
  ```

* for the `xdr_value_parser` target:

  ```bash
  cargo +nightly fuzz run xdr_value_parser -- -rss_limit_mb=4096
  ```

## Limitations

It would be much more efficient to run [structure-aware fuzzing](https://rust-fuzz.github.io/book/cargo-fuzz/structure-aware-fuzzing.html), which requires a custom corpus.

## References

* [https://github.com/rust-fuzz/cargo-fuzz]
* [https://rust-fuzz.github.io/book/cargo-fuzz.html]
