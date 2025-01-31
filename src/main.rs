use clap::{Parser, Subcommand};
use crc32fast::Hasher;
use std::fs;
use std::io;
use std::process;
use tokio::task::JoinSet;
use zip::read::ZipArchive;

// const PRINTABLE_CHARS: &[u8; 100] = b" 0123456789EeTtAaOoNnRrIiSsHhDdLlFfCcMmUuGgYyPpWwBbVvKkJjXxQqZz!\"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~\t\n\r\x0b\x0c";
const PRINTABLE_CHARS: &[u8; 27] = b"abcdefghijklmnopqrstuvwxyz_";

fn generator(mut id: u64, len: u8) -> Vec<u8> {
    let mut result = Vec::new();
    for _ in 0..len {
        result.push(PRINTABLE_CHARS[(id % PRINTABLE_CHARS.len() as u64) as usize]);
        id /= PRINTABLE_CHARS.len() as u64;
    }
    result
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

fn crc_collision(target_crc: u32, byte_count: usize) -> Vec<String> {
    // 仅使用可打印字符进行枚举

    fn calc_crc(data: &[u8]) -> u32 {
        let mut hasher = Hasher::new();
        hasher.update(data);
        hasher.finalize()
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut set = JoinSet::new();
        let len = PRINTABLE_CHARS.len().pow(byte_count as u32);
        let cores = num_cpus::get().max(2) - 1;
        let split_len = len / cores;

        for i in 0..cores {
            let start = i * split_len;
            let end = if i == cores - 1 {
                len
            } else {
                (i + 1) * split_len
            };
            let set = &mut set;
            set.spawn(async move {
                let mut possible_result = vec![];
                for i in start..end {
                    let data = generator(i as u64, byte_count as u8);
                    let crc = calc_crc(&data);
                    if crc == target_crc {
                        // println!("found! {:?}", data);
                        possible_result.push(String::from_utf8(data).unwrap());
                    }
                }
                possible_result
            });
        }
        let mut possible_result = vec![];

        while !set.is_empty() {
            if let Some(Ok(j)) = set.join_next().await {
                possible_result.extend(j);
            }
        }
        possible_result
    })
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

            for data in crc_collision(*crc, byte_count) {
                println!("[Success] 第 {} 个文件 {}: {:?}", i, crc, data);
            }
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
