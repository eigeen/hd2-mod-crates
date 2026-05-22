use crc32fast::Hasher;
use hd2_migrator_io::web::WebOutputFile;

const LOCAL_FILE_HEADER: u32 = 0x0403_4b50;
const CENTRAL_DIRECTORY_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;

#[derive(Debug, Clone)]
struct CentralEntry {
    path: String,
    crc32: u32,
    size: u32,
    local_offset: u32,
}

pub fn zip_store(files: &[WebOutputFile]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut central_entries = Vec::with_capacity(files.len());
    for file in files {
        let local_offset = output.len() as u32;
        let crc32 = crc32(&file.bytes);
        write_local_file(&mut output, &file.path, crc32, &file.bytes);
        central_entries.push(CentralEntry {
            path: file.path.clone(),
            crc32,
            size: file.bytes.len() as u32,
            local_offset,
        });
    }
    let central_offset = output.len() as u32;
    for entry in &central_entries {
        write_central_entry(&mut output, entry);
    }
    let central_size = output.len() as u32 - central_offset;
    write_end(
        &mut output,
        central_entries.len() as u16,
        central_size,
        central_offset,
    );
    output
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    hasher.finalize()
}

fn write_local_file(output: &mut Vec<u8>, path: &str, crc32: u32, bytes: &[u8]) {
    let name = path.as_bytes();
    write_u32(output, LOCAL_FILE_HEADER);
    write_u16(output, 20);
    write_u16(output, 0);
    write_u16(output, 0);
    write_u16(output, 0);
    write_u16(output, 0);
    write_u32(output, crc32);
    write_u32(output, bytes.len() as u32);
    write_u32(output, bytes.len() as u32);
    write_u16(output, name.len() as u16);
    write_u16(output, 0);
    output.extend_from_slice(name);
    output.extend_from_slice(bytes);
}

fn write_central_entry(output: &mut Vec<u8>, entry: &CentralEntry) {
    let name = entry.path.as_bytes();
    write_u32(output, CENTRAL_DIRECTORY_HEADER);
    write_u16(output, 20);
    write_u16(output, 20);
    write_u16(output, 0);
    write_u16(output, 0);
    write_u16(output, 0);
    write_u16(output, 0);
    write_u32(output, entry.crc32);
    write_u32(output, entry.size);
    write_u32(output, entry.size);
    write_u16(output, name.len() as u16);
    write_u16(output, 0);
    write_u16(output, 0);
    write_u16(output, 0);
    write_u16(output, 0);
    write_u32(output, 0);
    write_u32(output, entry.local_offset);
    output.extend_from_slice(name);
}

fn write_end(output: &mut Vec<u8>, count: u16, central_size: u32, central_offset: u32) {
    write_u32(output, END_OF_CENTRAL_DIRECTORY);
    write_u16(output, 0);
    write_u16(output, 0);
    write_u16(output, count);
    write_u16(output, count);
    write_u32(output, central_size);
    write_u32(output, central_offset);
    write_u16(output, 0);
}

fn write_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_zip_signatures_for_store_archive() {
        let zip = zip_store(&[WebOutputFile {
            path: "target/file.bin".to_string(),
            bytes: vec![1, 2, 3],
        }]);

        assert_eq!(&zip[0..4], &LOCAL_FILE_HEADER.to_le_bytes());
        assert!(
            zip.windows(4)
                .any(|window| window == CENTRAL_DIRECTORY_HEADER.to_le_bytes())
        );
        assert!(
            zip.windows(4)
                .any(|window| window == END_OF_CENTRAL_DIRECTORY.to_le_bytes())
        );
        assert!(zip.windows(15).any(|window| window == b"target/file.bin"));
    }
}
