use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};
use crate::model::{Capability, CapabilityRate, MetricInfo, Sample};

#[allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    dead_code,
    clippy::all
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

#[derive(Clone, Debug)]
pub enum BridgeEvent {
    Samples(Vec<Sample>),
    Message { level: String, text: String },
}

static EVENTS: Mutex<Option<SyncSender<BridgeEvent>>> = Mutex::new(None);

pub struct QcPerf {
    connected: Vec<u8>,
    capabilities: Vec<Capability>,
    warnings: Vec<String>,
}

impl QcPerf {
    pub fn init() -> Result<(Self, Receiver<BridgeEvent>)> {
        let (tx, rx) = sync_channel(256);
        *EVENTS.lock().expect("event bridge") = Some(tx);

        let init_rc = unsafe { bindings::qcperf_init() };
        if init_rc != bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_SUCCESS
            && init_rc != bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_ALREADY_INITIALIZED
        {
            *EVENTS.lock().expect("event bridge") = None;
            return Err(error_from(init_rc));
        }

        let mut qc = Self {
            connected: Vec::new(),
            capabilities: Vec::new(),
            warnings: Vec::new(),
        };

        let max = bindings::QcPerfBackendId::QC_PERF_BACKEND_MAX.0;
        for raw in 0..max {
            if raw > u8::MAX as _ {
                break;
            }
            let backend = bindings::QcPerfBackendId(raw);
            let rc = unsafe { bindings::qcperf_connect_backend(backend, Some(on_message)) };
            if rc == bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_INVALID_BACKEND_ID
                || rc == bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_NOT_SUPPORTED
            {
                continue;
            }
            if rc != bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_SUCCESS
                && rc != bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_BACKEND_ALREADY_CONNECTED
            {
                qc.warnings
                    .push(format!("backend {raw}: {}", error_from(rc)));
                continue;
            }

            let id = raw as u8;
            match unsafe { read_capabilities(backend, id) } {
                Ok(caps) => qc.capabilities.extend(caps),
                Err(err) => qc.warnings.push(err.to_string()),
            }

            let data_rc = unsafe { bindings::qcperf_set_data_callback(backend, Some(on_data)) };
            if data_rc != bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_SUCCESS
                && data_rc != bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_CALLBACK_ALREADY_SET
            {
                qc.warnings
                    .push(format!("backend {raw} data callback: {}", error_from(data_rc)));
            }
            qc.connected.push(id);
        }

        Ok((qc, rx))
    }

    pub fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn start(&mut self, rate: &CapabilityRate) -> Result<()> {
        let mut request = request_from(rate);
        let rc = unsafe { bindings::qcperf_start(backend_id(rate.backend_id), &mut request) };
        check(rc)
    }

    pub fn stop(&mut self, rate: &CapabilityRate) -> Result<()> {
        let mut request = request_from(rate);
        let rc = unsafe { bindings::qcperf_stop(backend_id(rate.backend_id), &mut request) };
        check(rc)
    }

    pub fn shutdown(self) -> Result<()> {
        let mut first_error = None;
        for id in self.connected.iter().rev() {
            let rc = unsafe { bindings::qcperf_disconnect_backend(backend_id(*id)) };
            if rc != bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_SUCCESS && first_error.is_none()
            {
                first_error = Some(error_from(rc));
            }
        }
        let rc = unsafe { bindings::qcperf_deinit() };
        *EVENTS.lock().expect("event bridge") = None;
        if rc != bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_SUCCESS
            && rc != bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_NOT_INITIALIZED
            && first_error.is_none()
        {
            first_error = Some(error_from(rc));
        }
        match first_error {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

pub fn try_recv(rx: &Receiver<BridgeEvent>) -> Option<BridgeEvent> {
    match rx.try_recv() {
        Ok(event) => Some(event),
        Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
    }
}

fn backend_id(raw: u8) -> bindings::QcPerfBackendId {
    bindings::QcPerfBackendId(raw as _)
}

fn request_from(rate: &CapabilityRate) -> bindings::QcPerfRequest {
    bindings::QcPerfRequest {
        capability_id: rate.capability_id,
        streaming_rate: rate.streaming_rate_ms,
        sampling_rate: rate.sampling_rate_ms,
    }
}

fn check(rc: bindings::QcPerfReturnCode) -> Result<()> {
    if rc == bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_SUCCESS {
        Ok(())
    } else {
        Err(error_from(rc))
    }
}

fn error_from(rc: bindings::QcPerfReturnCode) -> Error {
    Error::QcPerf {
        code: rc as i32,
        message: describe(rc),
    }
}

fn describe(rc: bindings::QcPerfReturnCode) -> String {
    unsafe {
        let mut info: bindings::QcPerfReturnCodeInfo = std::mem::zeroed();
        let lookup = bindings::qcperf_get_error_info(rc, &mut info);
        if lookup == bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_SUCCESS {
            let text = copy_text(&info.info_str, info.info_str_len);
            if text.is_empty() {
                format!("{rc:?}")
            } else {
                text
            }
        } else {
            format!("{rc:?}")
        }
    }
}

unsafe fn read_capabilities(
    backend: bindings::QcPerfBackendId,
    backend_id: u8,
) -> Result<Vec<Capability>> {
    let mut info: bindings::QcPerfBackendInfo = std::mem::zeroed();
    let rc = bindings::qcperf_get_capabilities_info(backend, &mut info);
    if rc != bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_SUCCESS {
        return Err(error_from(rc));
    }
    let count = info.capabilities_list_length as usize;
    if info.capabilities_list.is_null() || count == 0 {
        return Ok(Vec::new());
    }
    let caps = std::slice::from_raw_parts(info.capabilities_list, count);
    let mut owned = Vec::with_capacity(count);
    for cap in caps {
        owned.push(copy_capability(backend_id, cap));
    }
    Ok(owned)
}

fn copy_capability(backend_id: u8, cap: &bindings::QcPerfCapabilityInfo) -> Capability {
    let metric_len = cap.metric_ids_list_len as usize;
    let metrics = if cap.metric_ids_list.is_null() || metric_len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(cap.metric_ids_list, metric_len) }
            .iter()
            .map(copy_metric)
            .collect()
    };
    Capability {
        backend_id,
        capability_id: cap.capability_id,
        name: copy_text(&cap.capability_name, cap.capability_name_len),
        metrics,
        sampling_rates_ms: copy_rates(&cap.sampling_rate, cap.sampling_rate_len),
        streaming_rates_ms: copy_rates(&cap.streaming_rate, cap.streaming_rate_len),
    }
}

fn copy_metric(metric: &bindings::QcPerfMetricInfo) -> MetricInfo {
    MetricInfo {
        metric_id: metric.metric_id,
        name: copy_text(&metric.metric_name, metric.metric_name_len),
        description: copy_text(&metric.metric_description, metric.metric_description_len),
        unit: copy_text(&metric.metric_unit, metric.metric_unit_len),
    }
}

fn copy_rates(rates: &[u16], len: u8) -> Vec<u16> {
    let n = (len as usize).min(rates.len());
    rates[..n].to_vec()
}

fn copy_text<T: Copy>(bytes: &[T], len: usize) -> String {
    let width = std::mem::size_of::<T>();
    if width != 1 {
        return String::new();
    }
    let n = len.min(bytes.len());
    let mut raw = Vec::with_capacity(n);
    for item in bytes.iter().take(n) {
        let byte: u8 = unsafe { std::mem::transmute_copy(item) };
        if byte == 0 {
            break;
        }
        raw.push(byte);
    }
    String::from_utf8_lossy(&raw).into_owned()
}

fn publish(event: BridgeEvent) {
    let guard = EVENTS.lock().expect("event bridge");
    if let Some(sender) = guard.as_ref() {
        let _ = sender.try_send(event);
    }
}

unsafe extern "C" fn on_data(data: *mut bindings::QcPerfData) -> bindings::QcPerfReturnCode {
    let _ = std::panic::catch_unwind(|| {
        if let Some(samples) = copy_samples(data) {
            if !samples.is_empty() {
                publish(BridgeEvent::Samples(samples));
            }
        }
    });
    bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_SUCCESS
}

unsafe extern "C" fn on_message(
    message: *mut bindings::QcPerfMessage,
) -> bindings::QcPerfReturnCode {
    let _ = std::panic::catch_unwind(|| {
        if message.is_null() {
            return;
        }
        let message = &*message;
        let level = match message.message_level {
            bindings::QcPerfMessageLevel::QC_PERF_MESSAGE_LEVEL_ERROR => "error",
            bindings::QcPerfMessageLevel::QC_PERF_MESSAGE_LEVEL_WARNING => "warning",
            bindings::QcPerfMessageLevel::QC_PERF_MESSAGE_LEVEL_INFO => "info",
            bindings::QcPerfMessageLevel::QC_PERF_MESSAGE_LEVEL_DEBUG => "debug",
        };
        if level == "debug" || level == "info" || message.message.is_null() {
            return;
        }
        let len = message.message_length;
        let bytes = std::slice::from_raw_parts(message.message as *const u8, len);
        let text = String::from_utf8_lossy(bytes).trim().to_string();
        if !text.is_empty() {
            publish(BridgeEvent::Message {
                level: level.to_string(),
                text,
            });
        }
    });
    bindings::QcPerfReturnCode::QC_PERF_RETURN_CODE_SUCCESS
}

unsafe fn copy_samples(data: *mut bindings::QcPerfData) -> Option<Vec<Sample>> {
    if data.is_null() {
        return None;
    }
    let data = &*data;
    let count = data.metric_response_len as usize;
    if data.metric_response.is_null() || count == 0 {
        return Some(Vec::new());
    }
    let rows = std::slice::from_raw_parts(data.metric_response, count);
    let mut samples = Vec::with_capacity(count);
    for row in rows {
        let Some(value) = numeric_value(&row.metric_value) else {
            continue;
        };
        samples.push(Sample {
            at: sample_time(row.timestamp),
            backend_id: data.backend_id,
            capability_id: data.capabilityId,
            metric_id: row.metric_id,
            value,
        });
    }
    Some(samples)
}

fn numeric_value(value: &bindings::QcPerfGenericType) -> Option<f64> {
    match value.data_type {
        bindings::QcPerfDataType::QC_PERF_DATA_TYPE_BOOL => {
            Some(if value.bool_value { 1.0 } else { 0.0 })
        }
        bindings::QcPerfDataType::QC_PERF_DATA_TYPE_UINT64 => Some(value.uint64_value as f64),
        bindings::QcPerfDataType::QC_PERF_DATA_TYPE_INT64 => Some(value.int64_value as f64),
        bindings::QcPerfDataType::QC_PERF_DATA_TYPE_DOUBLE => Some(value.double_value),
        bindings::QcPerfDataType::QC_PERF_DATA_TYPE_STRING => None,
    }
}

fn sample_time(timestamp: u64) -> SystemTime {
    if timestamp == 0 {
        SystemTime::now()
    } else {
        UNIX_EPOCH
            .checked_add(Duration::from_millis(timestamp))
            .unwrap_or_else(SystemTime::now)
    }
}
