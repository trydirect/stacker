use brotli::CompressorWriter;
use std::io::Write;

pub fn compress(input: &str) -> Vec<u8> {
    let mut compressed = Vec::new();
    let mut compressor = CompressorWriter::new(&mut compressed, 4096, 11, 22);
    compressor.write_all(input.as_bytes()).unwrap();
    compressor.flush().unwrap();
    drop(compressor);
    compressed
}
