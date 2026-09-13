use ai::ScoutAi;
use ai_api::Observation;
fn main() {
    let observation = Observation {
        player: 0, tick: 0, width: 0, height: 0,
        tiles: vec![], entities: vec![],
    };
    assert_eq!(ScoutAi::default().decide(observation.view()), None);
}
