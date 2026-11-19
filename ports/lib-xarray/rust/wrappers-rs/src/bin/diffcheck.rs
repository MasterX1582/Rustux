use std::fs;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "cases.jsonl".to_string());
    let cases = fs::read_to_string(path).unwrap();
    wrappers_rs::port::set_trace(std::env::var("XATRACE").is_ok());
    for (ln, line) in cases.lines().enumerate() {
        let f = line
            .split("\"fn\": \"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let args_s = line
            .split("\"args\": [")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap();
        let args: Vec<i64> = args_s
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect();
        let ok: u64 = line
            .split("\"ok\": ")
            .nth(1)
            .unwrap()
            .trim()
            .trim_end_matches('}')
            .parse()
            .unwrap();
        let got = match f {
            "wrap_xa_seq" => {
                wrappers_rs::port::set_trace(std::env::var("XATRACE").is_ok());
                wrappers_rs::port::wrap_xa_seq(args[0] as u64, args[1], args[2])
            }
            "wrap_xa_multi" => wrappers_rs::port::wrap_xa_multi(args[0] as u64, args[1], args[2]),
            "wrap_xa_alloc" => wrappers_rs::port::wrap_xa_alloc(args[0] as u64, args[1], args[2]),
            "wrap_xa_iter" => wrappers_rs::port::wrap_xa_iter(args[0] as u64, args[1], args[2]),
            _ => panic!("{}", f),
        };
        if got != ok {
            println!(
                "MISMATCH line {} {} {:?} ok={} got={}",
                ln + 1,
                f,
                args,
                ok,
                got
            );
        } else if ln % 100 == 0 {
            println!("ok {}", ln);
        }
    }
    println!("done");
}
