use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

/// xorshift64* — tiny PRNG, seeded once from system entropy.
/// No external deps so release builds stay fast and portable.
struct Rng(u64);

impl Rng {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        let mut state = nanos ^ (std::process::id() as u64).rotate_left(32) ^ 0x9e37_79b9_7f4a_7c15;
        if state == 0 {
            state = 0x2545_f491_4f6c_dd1d;
        }
        Rng(state)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Uniform in [lo, hi].
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        if lo >= hi {
            return lo;
        }
        let span = hi - lo + 1;
        let zone = u64::MAX - (u64::MAX % span);
        loop {
            let v = self.next();
            if v < zone {
                return lo + v % span;
            }
        }
    }

    fn pick<'a>(&mut self, items: &'a [String]) -> &'a str {
        &items[self.range(0, items.len() as u64 - 1) as usize]
    }

    /// Fisher-Yates, in place.
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.range(0, i as u64) as usize;
            items.swap(i, j);
        }
    }
}

const USAGE: &str = "\
rcli — random things, on the command line

USAGE:
    rcli <COMMAND> [ARGS]

COMMANDS:
    roll <NdM>...        Roll dice (e.g. `roll 2d6 1d20`)
    flip [N]             Flip N coins (default 1), prints results
    pick <ITEM>...       Pick one item from the given list
    shuffle <ITEM>...    Shuffle the given items into a random order
    number [LO] [HI]     Random integer in [LO, HI] (default 1..100)
    password [LEN]       Random alphanumeric password (default 16)

OPTIONS:
    -h, --help           Print this help
    -V, --version        Print version
";

fn parse_dice(spec: &str) -> Result<(u64, u64), String> {
    let (n, m) = spec
        .split_once(['d', 'D'])
        .ok_or_else(|| format!("invalid dice spec `{spec}` (expected NdM, e.g. 2d6)"))?;
    let count: u64 = n.parse().map_err(|_| format!("invalid count in `{spec}`"))?;
    let sides: u64 = m.parse().map_err(|_| format!("invalid sides in `{spec}`"))?;
    if count == 0 || count > 10_000 {
        return Err(format!("dice count must be 1..=10000, got {count}"));
    }
    if sides < 2 {
        return Err(format!("dice must have at least 2 sides, got {sides}"));
    }
    Ok((count, sides))
}

fn cmd_roll(rng: &mut Rng, specs: &[String]) -> Result<(), String> {
    if specs.is_empty() {
        return Err("roll needs at least one dice spec, e.g. `roll 2d6`".into());
    }
    for spec in specs {
        let (count, sides) = parse_dice(spec)?;
        let rolls: Vec<u64> = (0..count).map(|_| rng.range(1, sides)).collect();
        let sum: u64 = rolls.iter().sum();
        println!("{spec}: {rolls:?} = {sum}");
    }
    Ok(())
}

fn cmd_flip(rng: &mut Rng, args: &[String]) -> Result<(), String> {
    let n: u64 = match args.first() {
        None => 1,
        Some(s) => s.parse().map_err(|_| format!("invalid count `{s}`"))?,
    };
    if n == 0 || n > 10_000 {
        return Err(format!("flip count must be 1..=10000, got {n}"));
    }
    let mut heads = 0u64;
    let mut out = String::new();
    for _ in 0..n {
        if rng.range(0, 1) == 0 {
            out.push('H');
            heads += 1;
        } else {
            out.push('T');
        }
    }
    println!("{out} ({heads} heads, {} tails)", n - heads);
    Ok(())
}

fn cmd_pick(rng: &mut Rng, items: &[String]) -> Result<(), String> {
    if items.is_empty() {
        return Err("pick needs at least one item, e.g. `pick pizza sushi tacos`".into());
    }
    println!("{}", rng.pick(items));
    Ok(())
}

fn cmd_shuffle(rng: &mut Rng, args: &[String]) -> Result<(), String> {
    if args.len() < 2 {
        return Err("shuffle needs at least two items, e.g. `shuffle alice bob carol`".into());
    }
    let mut items: Vec<&str> = args.iter().map(String::as_str).collect();
    rng.shuffle(&mut items);
    println!("{}", items.join(" "));
    Ok(())
}

fn cmd_number(rng: &mut Rng, args: &[String]) -> Result<(), String> {
    let (lo, hi) = match args.len() {
        0 => (1, 100),
        2 => (
            args[0].parse().map_err(|_| format!("invalid low bound `{}`", args[0]))?,
            args[1].parse().map_err(|_| format!("invalid high bound `{}`", args[1]))?,
        ),
        _ => return Err("number takes zero or two bounds, e.g. `number 1 6`".into()),
    };
    if lo > hi {
        return Err(format!("low bound {lo} exceeds high bound {hi}"));
    }
    println!("{}", rng.range(lo, hi));
    Ok(())
}

fn cmd_password(rng: &mut Rng, args: &[String]) -> Result<(), String> {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let len: u64 = match args.first() {
        None => 16,
        Some(s) => s.parse().map_err(|_| format!("invalid length `{s}`"))?,
    };
    if len == 0 || len > 1024 {
        return Err(format!("password length must be 1..=1024, got {len}"));
    }
    let mut out = String::with_capacity(len as usize);
    for _ in 0..len {
        out.push(ALPHABET[rng.range(0, ALPHABET.len() as u64 - 1) as usize] as char);
    }
    println!("{out}");
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let mut rng = Rng::new();
    match args.first().map(String::as_str) {
        Some("roll") => cmd_roll(&mut rng, &args[1..]),
        Some("flip") => cmd_flip(&mut rng, &args[1..]),
        Some("pick") => cmd_pick(&mut rng, &args[1..]),
        Some("shuffle") => cmd_shuffle(&mut rng, &args[1..]),
        Some("number") => cmd_number(&mut rng, &args[1..]),
        Some("password") => cmd_password(&mut rng, &args[1..]),
        Some("-h" | "--help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some("-V" | "--version") => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dice_specs_parse() {
        assert_eq!(parse_dice("2d6").unwrap(), (2, 6));
        assert_eq!(parse_dice("1D20").unwrap(), (1, 20));
        assert!(parse_dice("d6").is_err());
        assert!(parse_dice("2d0").is_err());
        assert!(parse_dice("2d").is_err());
    }

    #[test]
    fn range_stays_in_bounds() {
        let mut rng = Rng(42);
        for _ in 0..1000 {
            assert!((3..=7).contains(&rng.range(3, 7)));
        }
        assert_eq!(rng.range(5, 5), 5);
    }

    #[test]
    fn bad_commands_error() {
        assert!(run(&["bogus".into()]).is_err());
        assert!(run(&["roll".into()]).is_err());
        assert!(run(&["pick".into()]).is_err());
        assert!(run(&["shuffle".into()]).is_err());
        assert!(run(&["shuffle".into(), "only-one".into()]).is_err());
        assert!(run(&["number".into(), "9".into(), "1".into()]).is_err());
        assert!(run(&["number".into(), "1".into(), "6".into()]).is_ok());
    }

    #[test]
    fn shuffle_permutes_without_losing_items() {
        let mut rng = Rng(7);
        let mut items: Vec<u32> = (0..64).collect();
        rng.shuffle(&mut items);
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..64).collect::<Vec<u32>>(), "items were lost or duplicated");
        assert_ne!(items, sorted, "64 items should not land back in order");
    }
}
