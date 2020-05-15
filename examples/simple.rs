use miniclap::MiniClap;

#[derive(Debug, MiniClap)]
struct Opts {
    a: i64,
    b: String,
}

fn main() {
    let opts = Opts::parse();
    println!("opts = {:?}", opts);
}
