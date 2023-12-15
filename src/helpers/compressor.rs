use brotli::{CompressorWriter, Decompressor};
use std::io::{Read, Write};

pub fn compress(input: &str) -> Vec<u8> {
    let mut compressed = Vec::new();
    let mut compressor = CompressorWriter::new(
        &mut compressed, 4096, 11, 22
    );
    compressor.write_all(input.as_bytes()).unwrap();
    compressor.flush().unwrap();
    drop(compressor);
    compressed
}

pub fn decompress(input: &[u8]) -> String {
    let mut decompressed = String::new();
    let mut decompressor = Decompressor::new(input, 4096);
    decompressor.read_to_string(&mut decompressed).unwrap();
    decompressed
}
