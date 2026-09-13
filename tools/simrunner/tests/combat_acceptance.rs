use sim::{Command, CommandKind, Stance};
use simrunner::arena;

/// REQ: RM-M4-01
/// Automated portion only: readability still requires the Mac playtest.
#[test]
fn forty_per_side_finish_without_stranded_units_and_replay_identically() {
    let battle = arena::battle_40();
    assert!(battle.failure.is_none(), "{:?}", battle.failure);
    assert!(
        battle.winner().is_some(),
        "decisive result: {:?}",
        battle.survivors
    );
    assert_eq!(
        battle
            .replay
            .commands
            .iter()
            .filter(|(_, c)| matches!(c.kind, CommandKind::Spawn { .. }))
            .count(),
        80
    );
    let committed: sim::Replay = ron::from_str(include_str!(
        "../../../crates/sim/tests/corpus/battle-40v40.ron"
    ))
    .unwrap();
    assert_eq!(
        battle.replay, committed,
        "recipe and committed input must agree"
    );
    battle.replay.verify().expect("identical at every tick");
    println!(
        "40v40: {} ticks, survivors {:?}, longest inactivity {}",
        battle.replay.ticks, battle.survivors, battle.longest_inactivity
    );
}

/// REQ: GD-COMBAT-02
#[test]
fn the_two_explicit_counters_win_equal_budget_trials_on_both_sides() {
    for &(name, counter, target, budget) in arena::MATCHUPS {
        let rosters = [
            arena::roster_for_budget(counter, budget),
            arena::roster_for_budget(target, budget),
        ];
        let mut wins = [0u32; 2];
        for seed in 1..=u64::from(arena::DEFAULT_TRIALS) {
            for swapped in [false, true] {
                let battle = arena::finish(arena::setup(&rosters, seed, swapped));
                assert!(
                    battle.failure.is_none(),
                    "{name}, seed {seed}, swapped {swapped}: {:?}",
                    battle.failure
                );
                if battle.winner() == Some(u8::from(swapped)) {
                    wins[usize::from(swapped)] += 1;
                }
                println!(
                    "{name}, seed {seed}, swapped {swapped}: winner {:?}, survivors {:?}",
                    battle.winner(),
                    battle.survivors
                );
            }
        }
        assert!(
            wins.iter().all(|&n| n * 10 >= arena::DEFAULT_TRIALS * 9),
            "{name}: {wins:?} wins, need at least 90% on each side"
        );
    }
}

#[test]
fn inactivity_watchdog_rejects_a_fight_where_nobody_acts() {
    let mut sim = arena::setup(&arena::armies(), 1, false);
    for player in 0..2 {
        let ids = arena::living(&sim, player);
        sim.issue(Command {
            player,
            kind: CommandKind::Stop { ids: ids.clone() },
        });
        sim.issue(Command {
            player,
            kind: CommandKind::SetStance {
                ids,
                stance: Stance::Passive,
            },
        });
    }
    let battle = arena::finish(sim);
    assert!(
        battle
            .failure
            .as_deref()
            .is_some_and(|s| s.contains("inactive")),
        "{:?}",
        battle.failure
    );
    assert!(battle.replay.ticks < arena::BATTLE_LIMIT);
}

#[test]
fn balance_cli_checks_both_sides_and_rejects_empty_trials() {
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_simrunner"))
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&["balance", "--matches", "1"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("spearman-cavalry") && text.contains("slinger-infantry"));
    assert!(text.contains("1/1 left, 1/1 right"));
    assert_eq!(run(&["balance", "--matches", "0"]).status.code(), Some(2));
    assert_eq!(
        run(&[
            "balance",
            "--matches",
            "2",
            "--seed",
            "18446744073709551615"
        ])
        .status
        .code(),
        Some(2)
    );
}
