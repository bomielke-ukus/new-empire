// Nor does it re-export the simulation crate itself.
fn main() {
    let _ = fogged::sim::Simulation::new(1, Default::default());
}
