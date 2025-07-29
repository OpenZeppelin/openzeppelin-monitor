#![no_main]

use libfuzzer_sys::fuzz_target;
use openzeppelin_monitor::services::filter::expression::parse;

fuzz_target!(|data: &[u8]| {
    let expression_str = String::from_utf8_lossy(data);
    let _ = parse(&expression_str);
});
