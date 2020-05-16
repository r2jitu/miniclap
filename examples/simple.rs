use miniclap::MiniClap;

#[derive(Debug, MiniClap)]
struct Opts {
    #[miniclap(short = 'x')]
    a: i64,
    #[miniclap(short, long = "second")]
    b: String,
}

fn main() {
    let opts = Opts::parse();
    println!("opts = {:?}", opts);
}
