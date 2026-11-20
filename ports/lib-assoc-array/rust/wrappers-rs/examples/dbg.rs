fn main() {
    let a: Vec<String> = std::env::args().collect();
    let seed = a[1].parse::<i64>().unwrap() as u64;
    let n = a[2].parse::<i64>().unwrap();
    let s2 = a[3].parse::<i64>().unwrap();
    println!("{:016x}", wrappers_rs::port::wrap_assoc_seq(seed, n, s2));
}
