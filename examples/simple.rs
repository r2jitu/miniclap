use smallclap::SmallClap;

#[derive(Debug, SmallClap)]
struct Opts {
    a: i64,
    b: String,
}

fn main() {
    let opts = Opts::parse();
    println!("opts = {:?}", opts);
}
