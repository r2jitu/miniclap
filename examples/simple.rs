use miniclap::MiniClap;

#[derive(Debug, MiniClap)]
struct Opts {
    #[miniclap(short = "x", long)]
    first: bool,

    #[miniclap(short, long = "sec")]
    second: i64,

    pos: String,
}

fn main() {
    let opts = Opts::parse();
    println!("opts = {:?}", opts);
}
