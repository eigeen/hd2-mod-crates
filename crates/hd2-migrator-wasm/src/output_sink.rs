//! JS-backed output sink for moving serialized files out of WASM one at a time.

use crc32fast::Hasher;
use hd2_migrator_core::archive::{SerializedPart, StreamTocSerializer};
use hd2_migrator_core::operations::{PreparedArchiveOutput, WebOutputFile};
use js_sys::Function;
use std::io::{self, Write};
use wasm_bindgen::{JsCast, prelude::*};

const ARCHIVE_CHUNK_SIZE: usize = 4 * 1024 * 1024;

pub struct JsOutputSink {
    target: JsValue,
    on_file: Function,
}

pub struct JsArchiveOutputSink {
    target: JsValue,
    on_file_start: Function,
    on_file_chunk: Function,
    on_file_end: Function,
}

impl JsArchiveOutputSink {
    pub fn from_js(value: JsValue) -> Result<Self, JsValue> {
        if !value.is_object() {
            return Err(crate::error::js_error(
                "archive output sink must be an object",
            ));
        }
        Ok(Self {
            on_file_start: required_function(&value, "onFileStart")?,
            on_file_chunk: required_function(&value, "onFileChunk")?,
            on_file_end: required_function(&value, "onFileEnd")?,
            target: value,
        })
    }

    pub fn write_part(
        &self,
        path: &str,
        serializer: &StreamTocSerializer<'_>,
        part: SerializedPart,
    ) -> Result<(), JsValue> {
        let size = u32::try_from(serializer.part_len(part))
            .map_err(|_| crate::error::js_error("archive part exceeds the ZIP32 size limit"))?;
        let crc32 = calculate_part_crc(serializer, part)?;
        self.start_file(path, size, crc32)?;
        let mut writer = JsChunkWriter::new(self).map_err(crate::error::js_error)?;
        serializer
            .write_part(part, &mut writer)
            .map_err(crate::error::js_error)?;
        writer.flush().map_err(crate::error::js_error)?;
        self.on_file_end.call0(&self.target).map(|_| ())
    }

    pub fn write_archive(&self, output: &mut PreparedArchiveOutput) -> Result<(), JsValue> {
        let entries = output.manifest().entries().clone();
        let serializer = output.serializer();
        for entry in &entries {
            self.write_part(&entry.path, &serializer, entry.part)?;
        }
        Ok(())
    }

    fn start_file(&self, path: &str, size: u32, crc32: u32) -> Result<(), JsValue> {
        self.on_file_start
            .call3(
                &self.target,
                &JsValue::from_str(path),
                &JsValue::from_f64(f64::from(size)),
                &JsValue::from_f64(f64::from(crc32)),
            )
            .map(|_| ())
    }
}

fn required_function(value: &JsValue, key: &str) -> Result<Function, JsValue> {
    js_sys::Reflect::get(value, &JsValue::from_str(key))?
        .dyn_into::<Function>()
        .map_err(|_| crate::error::js_error(format!("outputSink.{key} must be a function")))
}

fn calculate_part_crc(
    serializer: &StreamTocSerializer<'_>,
    part: SerializedPart,
) -> Result<u32, JsValue> {
    let mut writer = CrcWriter::default();
    serializer
        .write_part(part, &mut writer)
        .map_err(crate::error::js_error)?;
    Ok(writer.finish())
}

#[derive(Default)]
struct CrcWriter {
    hasher: Hasher,
}

impl CrcWriter {
    fn finish(self) -> u32 {
        self.hasher.finalize()
    }
}

impl Write for CrcWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.hasher.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct JsChunkWriter<'a> {
    sink: &'a JsArchiveOutputSink,
    buffer: Vec<u8>,
}

impl<'a> JsChunkWriter<'a> {
    fn new(sink: &'a JsArchiveOutputSink) -> io::Result<Self> {
        let mut buffer = Vec::new();
        buffer
            .try_reserve_exact(ARCHIVE_CHUNK_SIZE)
            .map_err(|error| io::Error::other(error.to_string()))?;
        Ok(Self { sink, buffer })
    }

    fn flush_chunk(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let bytes = js_sys::Uint8Array::from(self.buffer.as_slice());
        self.sink
            .on_file_chunk
            .call1(&self.sink.target, &bytes)
            .map_err(|error| io::Error::other(format!("output chunk rejected: {error:?}")))?;
        self.buffer.clear();
        Ok(())
    }
}

impl Write for JsChunkWriter<'_> {
    fn write(&mut self, mut bytes: &[u8]) -> io::Result<usize> {
        let total = bytes.len();
        while !bytes.is_empty() {
            let available = ARCHIVE_CHUNK_SIZE - self.buffer.len();
            let copied = available.min(bytes.len());
            self.buffer.extend_from_slice(&bytes[..copied]);
            bytes = &bytes[copied..];
            if self.buffer.len() == ARCHIVE_CHUNK_SIZE {
                self.flush_chunk()?;
            }
        }
        Ok(total)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_chunk()
    }
}

impl JsOutputSink {
    pub fn from_js(value: JsValue) -> Result<Self, JsValue> {
        if !value.is_object() {
            return Err(crate::error::js_error("output sink must be an object"));
        }
        let on_file = js_sys::Reflect::get(&value, &JsValue::from_str("onFile"))?
            .dyn_into::<Function>()
            .map_err(|_| crate::error::js_error("outputSink.onFile must be a function"))?;
        Ok(Self {
            target: value,
            on_file,
        })
    }

    pub fn write(&self, file: WebOutputFile) -> Result<(), JsValue> {
        let crc = crc32fast::hash(&file.bytes);
        let bytes = js_sys::Uint8Array::from(file.bytes.as_slice());
        self.on_file
            .call3(
                &self.target,
                &JsValue::from_str(&file.path),
                &bytes,
                &JsValue::from_f64(crc as f64),
            )
            .map(|_| ())
    }
}
