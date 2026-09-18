// `fogged` re-exports the command and data types, not the simulation.
fn main() {
    let _ = fogged::Simulation::new(1, Default::default());
}
