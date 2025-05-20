use clap::{Parser, Subcommand};
use std::process;
use std::sync::LazyLock;
use std::{fs, io};
use zip::read::ZipArchive;

const PERMITTED_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ01234567890_";

const PERMITTED_CHARS_U8: &[u8; 64] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ01234567890_";

static IS_PERMITTED: LazyLock<Vec<bool>> = LazyLock::new(|| {
    let mut v = Vec::with_capacity(256);
    for b in 0u8..=255u8 {
        v.push(PERMITTED_CHARS.contains(b as char));
    }
    v
});

struct CRC32 {
    table: [u32; 256],
}

impl CRC32 {
    fn new(poly: u32) -> Self {
        let mut table = [0u32; 256];
        for (i, entry) in table.iter_mut().enumerate() {
            let mut v = i as u32;
            for _ in 0..8 {
                v = (v >> 1) ^ (poly & (v & 1).overflowing_neg().0);
            }
            *entry = v;
        }
        CRC32 { table }
    }

    fn _calc(&self, data: &[u8], mut accum: u32) -> u32 {
        accum = !accum;
        for &b in data {
            accum = self.table[((accum ^ b as u32) & 0xFF) as usize] ^ ((accum >> 8) & 0x00FFFFFF);
        }
        !accum
    }
}

struct CRC32Reverse {
    table: [u32; 256],
    table_reverse: Vec<Vec<u8>>,
}

impl CRC32Reverse {
    fn new(poly: u32) -> Self {
        let crc32 = CRC32::new(poly);
        let mut table_reverse = vec![Vec::new(); 256];
        for j in 0..256u16 {
            let idx = (crc32.table[j as usize] >> 24) as usize;
            table_reverse[idx].push(j as u8);
        }
        CRC32Reverse {
            table: crc32.table,
            table_reverse,
        }
    }
    fn calc(&self, data: &[u8], mut accum: u32) -> u32 {
        accum = !accum;
        for &b in data {
            accum = self.table[((accum ^ b as u32) & 0xFF) as usize] ^ ((accum >> 8) & 0x00FFFFFF);
        }
        !accum
    }

    fn _rewind(&self, data: &[u8], accum: u32) -> Vec<u32> {
        if data.is_empty() {
            return vec![accum];
        }
        let mut stack = vec![(data.len(), !accum)];
        let mut solutions = Vec::new();

        while let Some((offset, node)) = stack.pop() {
            let prev_offset = offset - 1;
            for &i in &self.table_reverse[((node >> 24) & 0xFF) as usize] {
                let prev_crc =
                    ((node ^ self.table[i as usize]) << 8) | (i as u32 ^ data[prev_offset] as u32);
                if prev_offset > 0 {
                    stack.push((prev_offset, prev_crc));
                } else {
                    solutions.push(!prev_crc);
                }
            }
        }
        solutions
    }

    fn find_reverse(&self, desired: u32, accum: u32) -> Vec<Vec<u8>> {
        let mut solutions = Vec::new();
        let accum = !accum;
        let mut stack = vec![((!desired), Vec::new())];

        while let Some((v, s)) = stack.pop() {
            for &j in &self.table_reverse[((v >> 24) & 0xFF) as usize] {
                let mut next_str = s.clone();
                next_str.push(j);
                if next_str.len() == 4 {
                    let mut a = accum;
                    let mut data = Vec::with_capacity(4);
                    for i in (0..4).rev() {
                        data.push((a ^ next_str[i] as u32) as u8);
                        a >>= 8;
                        a ^= self.table[next_str[i] as usize];
                    }
                    solutions.push(data);
                } else {
                    stack.push(((v ^ self.table[j as usize]) << 8, next_str));
                }
            }
        }
        solutions
    }
}

fn parse_dword(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
}

fn is_permitted(b: u8) -> bool {
    IS_PERMITTED[b as usize]
}

fn print_permitted_reverse(
    out: &mut Vec<String>,
    crc32_reverse: &CRC32Reverse,
    patch: &[u8],
    desired: u32,
    accum: u32,
) {
    let patches = crc32_reverse.find_reverse(desired, crc32_reverse.calc(patch, accum));
    for last_4_bytes in patches {
        if last_4_bytes.iter().all(|&b| is_permitted(b)) {
            let mut patch2 = patch.to_vec();
            patch2.extend_from_slice(&last_4_bytes);

            out.push(String::from_utf8_lossy(&patch2).to_string());
        }
    }
}

fn try_open_zip(path: &str) -> std::io::Result<ZipArchive<std::fs::File>> {
    match fs::File::open(path) {
        Ok(f) => match ZipArchive::new(f) {
            Ok(a) => Ok(a),
            Err(e) => {
                eprintln!("file is not a zip archive: {}", e);
                process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("file read error: {}", e);
            Err(e)
        }
    }
}

fn read_crc(zip_path: &str) -> io::Result<Vec<(String, u32, u64)>> {
    let mut archive: ZipArchive<fs::File> = try_open_zip(zip_path)?;
    let mut results = Vec::new();
    for i in 0..archive.len() {
        if let Ok(file) = archive.by_index_raw(i) {
            results.push((file.name().to_string(), file.crc32(), file.size()));
        }
    }
    Ok(results)
}

fn crc_collision(target_crc: u32, _byte_count: usize) -> Vec<String> {
    let poly = "0xEDB88320";
    let accum = "0";

    let crc32_reverse = CRC32Reverse::new(parse_dword(poly));
    let desired = target_crc;
    let accum = parse_dword(accum);

    // Original patch finding
    let patches = crc32_reverse.find_reverse(desired, accum);
    let mut out = vec![];
    for patch in patches {
        out.push(String::from_utf8_lossy(&patch).to_string());
    }

    // 5-byte alphanumeric patches
    for &i in PERMITTED_CHARS_U8 {
        print_permitted_reverse(&mut out, &crc32_reverse, &[i], desired, accum);
    }

    // 6-byte alphanumeric patches
    for &i in PERMITTED_CHARS_U8 {
        for &j in PERMITTED_CHARS_U8 {
            print_permitted_reverse(&mut out, &crc32_reverse, &[i, j], desired, accum);
        }
    }

    out.iter()
        .filter(|s| s.len() == _byte_count)
        .cloned()
        .collect()
}

fn handle_list_crc(file: &str) {
    if let Ok(file_info) = read_crc(file) {
        println!("reading!");
        for (i, (name, crc, size)) in file_info.iter().enumerate() {
            println!("[{}] {}: {} {}bytes", i, name, crc, size);
        }
    }
}

fn handle_crack_crc(file: &str, byte_count: usize) {
    println!("Cracking CRC32 collision for {}...", file);
    if let Ok(file_info) = read_crc(file) {
        for (i, (name, crc, size)) in file_info.iter().enumerate() {
            if size > &6u64 {
                continue;
            }
            println!("cracking {} file, size {}...", name, size);

            let data = crc_collision(*crc, byte_count);
            println!("[Success] 第 {} 个文件 0x{:x}: {:?}", i, crc, data);
        }
    }
}

#[derive(Parser)]
#[command(
    name = "CRC-Tools V2.2 (Rust)",
    version,
    about = "Use clap #[command] structure"
)]
struct Args {
    #[command(subcommand)]
    subcmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run brainfuck code
    #[command(alias = "l")]
    ListCrc {
        /// Input file (use - for stdin)
        file: String,
    },

    #[command(alias = "c")]
    CrackCrc {
        /// Input file (use - for stdin)
        file: String,

        #[arg(short, long, default_value = "4")]
        byte_count: usize,
    },
}

fn main() {
    let args = Args::parse();

    match args.subcmd {
        Commands::ListCrc { file } => {
            handle_list_crc(&file);
        }
        Commands::CrackCrc { file, byte_count } => {
            handle_crack_crc(&file, byte_count);
        }
    }
}
