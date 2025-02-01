use clap::{Parser, Subcommand};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::LazyLock;

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

    fn calc(&self, data: &[u8], mut accum: u32) -> u32 {
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

    fn rewind(&self, data: &[u8], accum: u32) -> Vec<u32> {
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

// struct Matrix {
//     matrix: Vec<u32>,
// }

// impl Matrix {
//     fn identity() -> Self {
//         Matrix {
//             matrix: (0..32).map(|i| 1 << i).collect(),
//         }
//     }

//     fn zero_operator(poly: u32) -> Self {
//         let mut m = vec![poly];
//         let mut n = 1;
//         for _ in 0..31 {
//             m.push(n);
//             n <<= 1;
//         }
//         Matrix { matrix: m }
//     }

//     fn multiply_vector(&self, v: u32, s: u32) -> u32 {
//         let mut result = s;
//         let mut v = v;
//         for &c in &self.matrix {
//             result ^= c & (0u32.wrapping_sub(v & 1));
//             v >>= 1;
//             if v == 0 {
//                 break;
//             }
//         }
//         result
//     }

//     fn mul(&self, other: &Matrix) -> Matrix {
//         Matrix {
//             matrix: other
//                 .matrix
//                 .iter()
//                 .map(|&v| self.multiply_vector(v, 0))
//                 .collect(),
//         }
//     }

//     fn sqr(&self) -> Matrix {
//         self.mul(self)
//     }
// }

// fn combine(c1: u32, c2: u32, l2: u32, n: u32, poly: u32) -> u32 {
//     let mut m = Matrix::zero_operator(poly);
//     m = m.sqr().sqr();

//     let mut mt = Matrix::identity();
//     let mut l2 = l2;
//     while l2 != 0 {
//         m = m.sqr();
//         if l2 & 1 != 0 {
//             mt = m.mul(&mt);
//         }
//         l2 >>= 1;
//     }

//     let mut b = c2;
//     let mut c1 = c1;
//     let mut n = n;
//     while n != 0 {
//         if n & 1 != 0 {
//             c1 = mt.multiply_vector(c1, b);
//         }
//         n >>= 1;
//         if n == 0 {
//             break;
//         }
//         b = mt.multiply_vector(b, b);
//         mt = mt.sqr();
//     }
//     c1
// }

// #[derive(Parser)]
// #[command(author, version, about = "CRC32 tools")]
// struct Cli {
//     #[command(subcommand)]
//     command: Commands,
// }

// #[derive(Subcommand)]
// enum Commands {
//     /// Output polynomial forms
//     Poly {
//         #[arg(default_value = "0xEDB88320")]
//         poly: String,
//         #[arg(long = "msbit", long = "normal")]
//         msb: bool,
//         #[arg(long)]
//         reciprocal: bool,
//         #[arg(short, long)]
//         output: Option<PathBuf>,
//     },
//     /// Generate lookup table
//     Table {
//         #[arg(default_value = "0xEDB88320")]
//         poly: String,
//         #[arg(short, long)]
//         output: Option<PathBuf>,
//     },
//     /// Calculate CRC of given data
//     Calc {
//         #[arg(default_value = "0")]
//         accum: String,
//         #[arg(default_value = "0xEDB88320")]
//         poly: String,
//         #[arg(long = "msbit", long = "normal")]
//         msb: bool,
//         #[arg(long)]
//         reciprocal: bool,
//         #[arg(short, long)]
//         input: Option<PathBuf>,
//         #[arg(short, long)]
//         input_str: Option<String>,
//         #[arg(short, long)]
//         output: Option<PathBuf>,
//     },
//     /// Find reverse CRC values
//     Reverse {
//         desired: String,
//         #[arg(default_value = "0")]
//         accum: String,
//         #[arg(default_value = "0xEDB88320")]
//         poly: String,
//         #[arg(long = "msbit", long = "normal")]
//         msb: bool,
//         #[arg(long)]
//         reciprocal: bool,
//         #[arg(short, long)]
//         output: Option<PathBuf>,
//     },
//     /// Undo CRC calculation
//     Undo {
//         accum: String,
//         #[arg(default_value = "0xEDB88320")]
//         poly: String,
//         #[arg(long = "msbit", long = "normal")]
//         msb: bool,
//         #[arg(long)]
//         reciprocal: bool,
//         #[arg(short, long)]
//         input: Option<PathBuf>,
//         #[arg(short, long)]
//         input_str: Option<String>,
//         #[arg(short, long = "len")]
//         rewind_len: Option<String>,
//         #[arg(short, long)]
//         output: Option<PathBuf>,
//     },
//     /// Combine CRC values
//     Combine {
//         accum: String,
//         checksum: String,
//         length: String,
//         #[arg(default_value = "1")]
//         n: String,
//         #[arg(default_value = "0xEDB88320")]
//         poly: String,
//         #[arg(long = "msbit", long = "normal")]
//         msb: bool,
//         #[arg(long)]
//         reciprocal: bool,
//         #[arg(short, long)]
//         output: Option<PathBuf>,
//     },
//     // ... other commands similar to Calc ...
// }

fn parse_dword(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
}

// fn print_num<W: Write>(out: &mut W, num: u32) -> io::Result<()> {
//     writeln!(out, "hex: 0x{:08x}", num)?;
//     writeln!(out, "dec: {}", num)?;
//     writeln!(out, "oct: 0o{:011o}", num)?;
//     writeln!(out, "bin: 0b{:032b}", num)?;
//     Ok(())
// }

// fn get_input(input: &Option<PathBuf>, input_str: &Option<String>) -> io::Result<Vec<u8>> {
//     if let Some(s) = input_str {
//         Ok(s.as_bytes().to_vec())
//     } else if let Some(path) = input {
//         let mut file = File::open(path)?;
//         let mut data = Vec::new();
//         file.read_to_end(&mut data)?;
//         Ok(data)
//     } else {
//         let mut data = Vec::new();
//         io::stdin().read_to_end(&mut data)?;
//         Ok(data)
//     }
// }

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
            // out.push(format!(
            //     "{} bytes: {} ({})",
            //     patch2.len(),
            //     String::from_utf8_lossy(&patch2),
            //     if crc32_reverse.calc(&patch2, accum) == desired {
            //         "OK"
            //     } else {
            //         "ERROR"
            //     }
            // ));
            out.push(String::from_utf8_lossy(&patch2).to_string());
        }
    }
}

// fn main() -> io::Result<()> {
//     let cli = Cli::parse();

//     match cli.command {
//         Commands::Poly {
//             poly: _,
//             msb: _,
//             reciprocal: _,
//             output,
//         } => {
//             let out: Box<dyn Write> = if let Some(path) = output {
//                 Box::new(File::create(path)?)
//             } else {
//                 Box::new(io::stdout())
//             };
//             // Handle poly command...
//             // let poly_val = get_poly(&poly, msb, reciprocal, &mut out)?;
//             // writeln!(out, "Reversed (lsbit-first)")?;
//             // print_num(&mut out, poly_val)?;
//             // writeln!(out, "Normal (msbit-first)")?;
//             // print_num(&mut out, reverse_bits(poly_val))?;
//             // let r = reciprocal(poly_val)?;
//             // writeln!(out, "Reversed reciprocal (Koopman notation)")?;
//             // print_num(&mut out, reverse_bits(r))?;
//             // writeln!(out, "Reciprocal")?;
//             // print_num(&mut out, r)?;
//         }
//         Commands::Table { poly, output } => {
//             let mut out: Box<dyn Write> = if let Some(path) = output {
//                 Box::new(File::create(path)?)
//             } else {
//                 Box::new(io::stdout())
//             };
//             let crc32 = CRC32::new(parse_dword(&poly));
//             write!(out, "[")?;
//             for (i, &v) in crc32.table.iter().enumerate() {
//                 if i > 0 {
//                     write!(out, ", ")?;
//                 }
//                 write!(out, "0x{:08x}", v)?;
//             }
//             writeln!(out, "]")?;
//         }
//         Commands::Calc {
//             accum,
//             poly,
//             msb: _,
//             reciprocal: _,
//             input,
//             input_str,
//             output,
//         } => {
//             let mut out: Box<dyn Write> = if let Some(path) = output {
//                 Box::new(File::create(path)?)
//             } else {
//                 Box::new(io::stdout())
//             };
//             let data = get_input(&input, &input_str)?;
//             let crc32 = CRC32::new(parse_dword(&poly));
//             writeln!(out, "data len: {}", data.len())?;
//             writeln!(out)?;
//             print_num(&mut out, crc32.calc(&data, parse_dword(&accum)))?;
//         }
//         Commands::Reverse {
//             desired,
//             accum,
//             poly,
//             msb,
//             reciprocal: rec,
//             output,
//         } => {
//             let mut out: Box<dyn Write> = if let Some(path) = output {
//                 Box::new(File::create(path)?)
//             } else {
//                 Box::new(io::stdout())
//             };

//             let crc32_reverse = CRC32Reverse::new(parse_dword(&poly));
//             let desired = parse_dword(&desired);
//             let accum = parse_dword(&accum);

//             // Original patch finding
//             let patches = crc32_reverse.find_reverse(desired, accum);
//             for patch in patches {
//                 writeln!(out, "4 bytes: {:?}", patch)?;
//                 let checksum = crc32_reverse.calc(&patch, accum);
//                 writeln!(
//                     out,
//                     "verification checksum: 0x{:08x} ({})",
//                     checksum,
//                     if checksum == desired { "OK" } else { "ERROR" }
//                 )?;
//             }

//             // 5-byte alphanumeric patches
//             for &i in PERMITTED_CHARS_U8 {
//                 print_permitted_reverse(&mut out, &crc32_reverse, &[i], desired, accum)?;
//             }

//             // 6-byte alphanumeric patches
//             for &i in PERMITTED_CHARS_U8 {
//                 for &j in PERMITTED_CHARS_U8 {
//                     print_permitted_reverse(&mut out, &crc32_reverse, &[i, j], desired, accum)?;
//                 }
//             }
//         }
//         Commands::Undo {
//             accum,
//             poly,
//             msb,
//             reciprocal: rec,
//             input,
//             input_str,
//             rewind_len,
//             output,
//         } => {
//             let mut out: Box<dyn Write> = if let Some(path) = output {
//                 Box::new(File::create(path)?)
//             } else {
//                 Box::new(io::stdout())
//             };

//             let crc32_reverse = CRC32Reverse::new(parse_dword(&poly));
//             let accum = parse_dword(&accum);
//             let data = get_input(&input, &input_str)?;
//             let maxlen = rewind_len
//                 .as_ref()
//                 .map(|s| s.parse::<usize>().unwrap_or(data.len()))
//                 .unwrap_or(data.len());

//             writeln!(
//                 out,
//                 "rewinded {}/{} ({:.2}%)",
//                 maxlen,
//                 data.len(),
//                 maxlen as f64 * 100.0 / data.len() as f64
//             )?;

//             for solution in crc32_reverse.rewind(&data[data.len() - maxlen..], accum) {
//                 writeln!(out)?;
//                 print_num(&mut out, solution)?;
//             }
//         }
//         Commands::Combine {
//             accum,
//             checksum,
//             length,
//             n,
//             poly,
//             msb,
//             reciprocal: rec,
//             output,
//         } => {
//             let mut out: Box<dyn Write> = if let Some(path) = output {
//                 Box::new(File::create(path)?)
//             } else {
//                 Box::new(io::stdout())
//             };

//             let c1 = parse_dword(&accum);
//             let c2 = parse_dword(&checksum);
//             let l2 = parse_dword(&length);
//             let n = n.parse::<u32>().unwrap_or(1);

//             print_num(&mut out, combine(c1, c2, l2, n, parse_dword(&poly)))?;
//         }
//     }
//     Ok(())
// }

use crc32fast::Hasher;
use std::fs;
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
    let poly = "0xEDB88320";
    let accum = "0";

    let crc32_reverse = CRC32Reverse::new(parse_dword(poly));
    let desired = target_crc;
    let accum = parse_dword(accum);

    // Original patch finding
    let patches = crc32_reverse.find_reverse(desired, accum);
    let mut out = vec![];
    for patch in patches {
        // out.push(format!(
        //     "4 bytes: {}",
        //     patch.iter().map(|&b| b as char).collect::<String>()
        // ));
        out.push(String::from_utf8_lossy(&patch).to_string());
        // let checksum = crc32_reverse.calc(&patch, accum);
        // out.push(format!(
        //     "verification checksum: 0x{:08x} ({})",
        //     checksum,
        //     if checksum == desired { "OK" } else { "ERROR" }
        // ));
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

    out

    // 仅使用可打印字符进行枚举

    // fn calc_crc(data: &[u8]) -> u32 {
    //     let mut hasher = Hasher::new();
    //     hasher.update(data);
    //     hasher.finalize()
    // }

    // let rt = tokio::runtime::Runtime::new().unwrap();
    // rt.block_on(async {
    //     let mut set = JoinSet::new();
    //     let len = PRINTABLE_CHARS.len().pow(byte_count as u32);
    //     let cores = num_cpus::get().max(2) - 1;
    //     let split_len = len / cores;

    //     for i in 0..cores {
    //         let start = i * split_len;
    //         let end = if i == cores - 1 {
    //             len
    //         } else {
    //             (i + 1) * split_len
    //         };
    //         let set = &mut set;
    //         set.spawn(async move {
    //             let mut possible_result = vec![];
    //             for i in start..end {
    //                 let data = generator(i as u64, byte_count as u8);
    //                 let crc = calc_crc(&data);
    //                 if crc == target_crc {
    //                     // println!("found! {:?}", data);
    //                     possible_result.push(String::from_utf8(data).unwrap());
    //                 }
    //             }
    //             possible_result
    //         });
    //     }
    //     let mut possible_result = vec![];

    //     while !set.is_empty() {
    //         if let Some(Ok(j)) = set.join_next().await {
    //             possible_result.extend(j);
    //         }
    //     }
    //     possible_result
    // })
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
