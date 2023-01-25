use mind::encoding::Node;
use std::io::stdin;

fn main() {
  let node: Result<Node, _> = serde_json::from_reader(stdin());

  println!("{node:#?}");
}
