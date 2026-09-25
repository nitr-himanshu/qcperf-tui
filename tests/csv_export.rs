use std::time::{Duration, SystemTime};

use qcperf_tui::export::csv::{self, HEADER};
use qcperf_tui::model::Sample;

#[test]
fn csv_header_and_one_data_row() {
    let sample = Sample {
        at: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        backend_id: 1,
        capability_id: 2,
        metric_id: 3,
        value: 12.5,
    };
    let row = csv::format_row("Overview", "util", "%", &sample);
    assert_eq!(
        HEADER,
        "timestamp_rfc3339,dashboard,backend_id,capability_id,metric_id,metric_name,value,unit"
    );
    assert!(row.contains("Overview,1,2,3,util,12.5,%"));
    assert!(row.starts_with("202"));
}
