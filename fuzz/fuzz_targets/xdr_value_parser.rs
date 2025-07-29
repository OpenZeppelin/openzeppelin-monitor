#![no_main]

use libfuzzer_sys::fuzz_target;
use openzeppelin_monitor::services::filter::stellar_helpers::parse_xdr_value;

fuzz_target!(|data: &[u8]| {
    let _ = parse_xdr_value(data, false);
});
