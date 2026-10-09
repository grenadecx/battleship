use super::*;
use crate::domain::ShipKind;
use crate::test_support::{c, open_water, row_fleet, ship_squares, sunk};

fn ready_session(i_go_first: bool) -> Session {
    let mut s = Session::new(i_go_first);
    s.set_board(row_fleet()).unwrap();
    s.ready().unwrap();
    s.opponent_ready().unwrap();
    s
}

/// Our turn again, after we missed at `target` and the opponent missed at J10.
fn after_we_missed_at(target: Coord) -> Session {
    let mut s = ready_session(true);
    s.fire(target).unwrap();
    s.receive_result(ShotResult::Miss).unwrap();
    s.receive_fire(c(9, 9)).unwrap();
    s
}

/// The opponent sinks our whole fleet while every one of our shots misses.
fn lose(s: &mut Session) {
    let mut our_shots = Coord::all();
    let mut their_shots = ship_squares(s.my_board()).into_iter();
    while !s.is_over() {
        if s.phase() == Phase::MyTurn {
            s.fire(our_shots.next().unwrap()).unwrap();
            s.receive_result(ShotResult::Miss).unwrap();
        }
        s.receive_fire(their_shots.next().unwrap()).unwrap();
    }
}

/// We sink the opponent's fleet (laid out like ours) while every one of their shots misses.
fn win(s: &mut Session) {
    let mut enemy = row_fleet();
    let mut our_shots = ship_squares(&enemy).into_iter();
    let mut their_shots = open_water(s.my_board()).into_iter();
    while !s.is_over() {
        if s.phase() == Phase::TheirTurn {
            s.receive_fire(their_shots.next().unwrap()).unwrap();
        }
        let target = our_shots.next().unwrap();
        s.fire(target).unwrap();
        s.receive_result(enemy.fire(target).unwrap()).unwrap();
    }
}

fn lost_session(i_go_first: bool) -> Session {
    let mut s = ready_session(i_go_first);
    lose(&mut s);
    s
}

#[test]
fn starts_in_placement() {
    assert_eq!(Session::new(true).phase(), Phase::Placement);
}

#[test]
fn cannot_be_ready_with_an_incomplete_fleet() {
    assert_eq!(
        Session::new(true).ready(),
        Err(SessionError::FleetIncomplete)
    );
}

#[test]
fn stays_in_placement_when_getting_ready_fails() {
    let mut s = Session::new(true);
    let _ = s.ready();
    assert_eq!(s.phase(), Phase::Placement);
}

#[test]
fn waits_for_the_opponent_after_getting_ready() {
    let mut s = Session::new(true);
    s.set_board(row_fleet()).unwrap();
    s.ready().unwrap();
    assert_eq!(s.phase(), Phase::WaitingForOpponent);
}

#[test]
fn we_open_when_we_go_first() {
    assert_eq!(ready_session(true).phase(), Phase::MyTurn);
}

#[test]
fn they_open_when_they_go_first() {
    assert_eq!(ready_session(false).phase(), Phase::TheirTurn);
}

#[test]
fn opponent_ready_first_does_not_end_our_placement() {
    let mut s = Session::new(false);
    s.opponent_ready().unwrap();
    assert_eq!(s.phase(), Phase::Placement);
}

#[test]
fn game_starts_when_we_get_ready_after_the_opponent() {
    let mut s = Session::new(false);
    s.opponent_ready().unwrap();
    s.set_board(row_fleet()).unwrap();
    s.ready().unwrap();
    assert_eq!(s.phase(), Phase::TheirTurn);
}

#[test]
fn board_cannot_change_after_getting_ready() {
    assert_eq!(
        ready_session(true).set_board(row_fleet()),
        Err(SessionError::NotAllowedNow(Phase::MyTurn))
    );
}

#[test]
fn firing_waits_for_the_result() {
    let mut s = ready_session(true);
    s.fire(c(4, 4)).unwrap();
    assert_eq!(s.phase(), Phase::AwaitingResult(c(4, 4)));
}

#[test]
fn cannot_fire_on_their_turn() {
    assert_eq!(
        ready_session(false).fire(c(4, 4)),
        Err(SessionError::NotAllowedNow(Phase::TheirTurn))
    );
}

#[test]
fn cannot_fire_during_placement() {
    assert_eq!(
        Session::new(true).fire(c(4, 4)),
        Err(SessionError::NotAllowedNow(Phase::Placement))
    );
}

#[test]
fn cannot_fire_twice_at_the_same_square() {
    assert_eq!(
        after_we_missed_at(c(4, 4)).fire(c(4, 4)),
        Err(SessionError::AlreadyTargeted(c(4, 4)))
    );
}

#[test]
fn repeated_shot_keeps_our_turn() {
    let mut s = after_we_missed_at(c(4, 4));
    let _ = s.fire(c(4, 4));
    assert_eq!(s.phase(), Phase::MyTurn);
}

#[test]
fn turn_passes_after_our_shot_is_reported() {
    let mut s = ready_session(true);
    s.fire(c(4, 4)).unwrap();
    s.receive_result(ShotResult::Hit).unwrap();
    assert_eq!(s.phase(), Phase::TheirTurn);
}

#[test]
fn reported_hit_is_recorded_on_the_enemy_grid() {
    let mut s = ready_session(true);
    s.fire(c(4, 4)).unwrap();
    s.receive_result(ShotResult::Hit).unwrap();
    assert_eq!(s.enemy_grid().open_hits(), vec![c(4, 4)]);
}

#[test]
fn result_without_a_shot_is_rejected() {
    assert_eq!(
        ready_session(false).receive_result(ShotResult::Miss),
        Err(SessionError::NotAllowedNow(Phase::TheirTurn))
    );
}

#[test]
fn inconsistent_sunk_report_is_rejected() {
    let mut s = ready_session(true);
    s.fire(c(4, 4)).unwrap();
    assert!(matches!(
        s.receive_result(sunk(ShipKind::Destroyer, &[c(0, 0), c(1, 0)])),
        Err(SessionError::BadReport(_))
    ));
}

#[test]
fn incoming_fire_is_resolved_against_our_board() {
    assert_eq!(
        ready_session(false).receive_fire(c(0, 0)),
        Ok(ShotResult::Hit)
    );
}

#[test]
fn incoming_fire_is_marked_on_our_board() {
    let mut s = ready_session(false);
    s.receive_fire(c(0, 0)).unwrap();
    assert!(s.my_board().was_shot(c(0, 0)));
}

#[test]
fn incoming_fire_passes_the_turn() {
    let mut s = ready_session(false);
    s.receive_fire(c(0, 0)).unwrap();
    assert_eq!(s.phase(), Phase::MyTurn);
}

#[test]
fn incoming_fire_on_our_turn_is_rejected() {
    assert_eq!(
        ready_session(true).receive_fire(c(0, 0)),
        Err(SessionError::NotAllowedNow(Phase::MyTurn))
    );
}

#[test]
fn repeated_incoming_fire_is_rejected() {
    let mut s = ready_session(false);
    s.receive_fire(c(9, 9)).unwrap();
    s.fire(c(0, 0)).unwrap();
    s.receive_result(ShotResult::Miss).unwrap();
    assert_eq!(
        s.receive_fire(c(9, 9)),
        Err(SessionError::AlreadyTargeted(c(9, 9)))
    );
}

#[test]
fn losing_the_last_ship_loses_the_game() {
    assert_eq!(lost_session(false).phase(), Phase::Lost);
}

#[test]
fn lost_game_is_over() {
    assert!(lost_session(false).is_over());
}

#[test]
fn sinking_the_last_enemy_ship_wins_the_game() {
    let mut s = ready_session(true);
    win(&mut s);
    assert_eq!(s.phase(), Phase::Won);
}

#[test]
fn new_round_is_only_possible_after_the_game() {
    assert_eq!(
        ready_session(true).new_round(),
        Err(SessionError::NotAllowedNow(Phase::MyTurn))
    );
}

#[test]
fn new_round_returns_to_placement() {
    let mut s = lost_session(true);
    s.new_round().unwrap();
    assert_eq!(s.phase(), Phase::Placement);
}

#[test]
fn new_round_clears_our_board() {
    let mut s = lost_session(true);
    s.new_round().unwrap();
    assert!(s.my_board().placements().is_empty());
}

#[test]
fn new_round_clears_the_enemy_grid() {
    let mut s = lost_session(true);
    s.new_round().unwrap();
    assert!(s.enemy_grid().sunk_ships().is_empty());
}

#[test]
fn new_round_is_opened_by_the_other_player() {
    let mut s = lost_session(true);
    s.new_round().unwrap();
    s.set_board(row_fleet()).unwrap();
    s.ready().unwrap();
    s.opponent_ready().unwrap();
    assert_eq!(s.phase(), Phase::TheirTurn);
}

#[test]
fn opponent_can_get_ready_for_the_next_round_before_we_leave_the_result_screen() {
    let mut s = lost_session(false);
    s.opponent_ready().unwrap();
    s.new_round().unwrap();
    s.set_board(row_fleet()).unwrap();
    s.ready().unwrap();
    assert_eq!(s.phase(), Phase::MyTurn);
}

#[test]
fn readiness_from_the_previous_round_does_not_carry_over() {
    let mut s = lost_session(true);
    s.new_round().unwrap();
    s.set_board(row_fleet()).unwrap();
    s.ready().unwrap();
    assert_eq!(s.phase(), Phase::WaitingForOpponent);
}

#[test]
fn opponent_ready_mid_battle_is_rejected() {
    assert_eq!(
        ready_session(true).opponent_ready(),
        Err(SessionError::NotAllowedNow(Phase::MyTurn))
    );
}

#[test]
fn errors_explain_themselves() {
    for (error, text) in [
        (
            SessionError::NotAllowedNow(Phase::MyTurn),
            "not allowed during MyTurn",
        ),
        (
            SessionError::FleetIncomplete,
            "the fleet is not fully placed",
        ),
        (
            SessionError::AlreadyTargeted(c(1, 2)),
            "B3 was already targeted",
        ),
        (
            SessionError::BadReport("nonsense".into()),
            "bad report: nonsense",
        ),
    ] {
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn remembers_who_goes_first() {
    assert!(Session::new(true).i_go_first());
    assert!(!Session::new(false).i_go_first());
}
