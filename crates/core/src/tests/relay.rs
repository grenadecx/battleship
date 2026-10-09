use super::*;

#[test]
fn requests_round_trip() {
    for (request, line) in [
        (Request::Host, "HOST"),
        (Request::Join("K7QD".into()), "JOIN K7QD"),
    ] {
        assert_eq!(request.encode(), line);
        assert_eq!(Request::decode(line), Ok(request));
    }
}

#[test]
fn replies_round_trip() {
    for (reply, line) in [
        (Reply::Room("K7QD".into()), "ROOM K7QD"),
        (Reply::Paired, "PAIRED"),
        (
            Reply::Error("No game with code K7QD".into()),
            "ERROR No game with code K7QD",
        ),
    ] {
        assert_eq!(reply.encode(), line);
        assert_eq!(Reply::decode(line), Ok(reply));
    }
}

#[test]
fn join_codes_are_normalized_on_decoding() {
    assert_eq!(
        Request::decode("JOIN k7qd"),
        Ok(Request::Join("K7QD".into()))
    );
}

#[test]
fn tolerates_surrounding_whitespace() {
    assert_eq!(Request::decode(" HOST \r\n"), Ok(Request::Host));
    assert_eq!(Reply::decode("PAIRED\n"), Ok(Reply::Paired));
}

#[test]
fn rejects_garbage_requests() {
    for line in ["", "HOST now", "JOIN", "JOIN A B", "HELLO BATTLESHIP 2"] {
        assert!(Request::decode(line).is_err(), "accepted {line:?}");
    }
}

#[test]
fn rejects_garbage_replies() {
    for line in ["", "ROOM", "PAIRED up", "READY"] {
        assert!(Reply::decode(line).is_err(), "accepted {line:?}");
    }
}

#[test]
fn new_codes_use_the_unambiguous_alphabet() {
    let mut rng = Rng::new(1);
    for _ in 0..200 {
        let code = new_code(&mut rng);
        assert_eq!(code.len(), CODE_LENGTH);
        assert!(code.bytes().all(|b| CODE_ALPHABET.contains(&b)), "{code}");
    }
}

#[test]
fn new_codes_vary() {
    let mut rng = Rng::new(2);
    let codes: std::collections::HashSet<String> = (0..50).map(|_| new_code(&mut rng)).collect();
    assert!(codes.len() > 45, "only {} distinct codes", codes.len());
}

#[test]
fn normalizing_uppercases_and_drops_everything_but_letters_and_digits() {
    assert_eq!(normalize_code(" k7-qd "), "K7QD");
}

#[test]
fn normalizing_cuts_codes_to_length() {
    assert_eq!(normalize_code("ABCDEFG"), "ABCD");
}
