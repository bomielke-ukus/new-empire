// The simulation crate is not a dependency of `ai`: this does not compile.
fn main() {
    let world: Option<sim::World> = None;
    let _ = world;
}
