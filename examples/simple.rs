use miniclap::MiniClap;

#[derive(Debug, MiniClap)]
struct Opts {
    #[miniclap(short = "x", long)]
    first: bool,

    #[miniclap(short, long = "sec", default_value = 100)]
    second: Vec<i64>,

    pos: String,

    count: u8,
}

fn main() {
    let opts = Opts::parse_or_exit();
    println!("opts = {:?}", opts);
}
