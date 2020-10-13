extern crate crossbeam;

use std::collections::VecDeque;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Result as IOResult};

use crossbeam::{channel, thread};

const DECK_SIZE: usize = 52;
const HAND_SIZE: usize = 26;

const TURN_CUTOFF: usize = 10000;

const CACHE_FILE: &'static str = "test_cache.txt";
const PERMUTATION_FILE: &'static str = "permutations.txt";

static mut HASHES_TO_TEST: Vec<String> = Vec::new();

#[repr(usize)]
#[derive(Copy, Clone, PartialEq, Eq)]
enum Card {
    Blank = 0,
    Jack = 1,
    Queen = 2,
    King = 3,
    Ace = 4,
}

impl Card {
    #[inline]
    const fn from_char(c: char) -> Self {
        match c {
            'J' => Card::Jack,
            'Q' => Card::Queen,
            'K' => Card::King,
            'A' => Card::Ace,
            _ => Card::Blank,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum GameState {
    Player1Draw,
    Player2Draw,
    Player1Pay(usize),
    Player2Pay(usize),
}

#[derive(Clone, PartialEq, Eq)]
struct ThreadData {
    most_turns: usize,
    least_turns: usize,
    suspicious_games: usize,
    most_hash: String,
    least_hash: String,
}

fn get_players_from_hash(hash: &str, player1: &mut VecDeque<Card>, player2: &mut VecDeque<Card>) {
    for c in hash.chars().take(HAND_SIZE) {
        player1.push_back(Card::from_char(c));
    }

    for c in hash.chars().skip(HAND_SIZE).take(HAND_SIZE) {
        player2.push_back(Card::from_char(c));
    }
}

fn simulate_game(
    player1: &mut VecDeque<Card>,
    player2: &mut VecDeque<Card>,
    discard_pile: &mut VecDeque<Card>,
) -> (usize, bool) {
    let mut turns = 0;
    let mut suspicious = false;
    let mut game_state = GameState::Player1Draw;

    'game_loop: loop {
        turns += 1;

        match game_state {
            GameState::Player1Draw => {
                if let Some(card) = player1.pop_front() {
                    discard_pile.push_front(card);

                    game_state = if card == Card::Blank {
                        if player1.len() > 0 {
                            GameState::Player2Draw
                        } else {
                            break 'game_loop;
                        }
                    } else {
                        GameState::Player2Pay(card as usize)
                    };
                } else {
                    break 'game_loop;
                }
            }
            GameState::Player2Draw => {
                if let Some(card) = player2.pop_front() {
                    discard_pile.push_front(card);

                    game_state = if card == Card::Blank {
                        if player2.len() > 0 {
                            GameState::Player1Draw
                        } else {
                            break 'game_loop;
                        }
                    } else {
                        GameState::Player1Pay(card as usize)
                    };
                } else {
                    break 'game_loop;
                }
            }
            GameState::Player1Pay(amount) => {
                let mut paid_in_full = true;

                'p1_pay_loop: for _ in 0..amount {
                    if let Some(card) = player1.pop_front() {
                        discard_pile.push_front(card);

                        if card == Card::Blank {
                            if player1.len() > 0 {
                                continue 'p1_pay_loop;
                            } else {
                                break 'game_loop;
                            }
                        } else {
                            paid_in_full = false;
                            game_state = GameState::Player2Pay(card as usize);
                            break 'p1_pay_loop;
                        }
                    } else {
                        break 'game_loop;
                    }
                }

                if paid_in_full {
                    game_state = GameState::Player2Draw;

                    while let Some(card) = discard_pile.pop_back() {
                        player2.push_back(card);
                    }
                }
            }
            GameState::Player2Pay(amount) => {
                let mut paid_in_full = true;

                'p2_pay_loop: for _ in 0..amount {
                    if let Some(card) = player2.pop_front() {
                        discard_pile.push_front(card);

                        if card == Card::Blank {
                            if player2.len() > 0 {
                                continue 'p2_pay_loop;
                            } else {
                                break 'game_loop;
                            }
                        } else {
                            paid_in_full = false;
                            game_state = GameState::Player1Pay(card as usize);
                            break 'p2_pay_loop;
                        }
                    } else {
                        break 'game_loop;
                    }
                }

                if paid_in_full {
                    game_state = GameState::Player1Draw;

                    while let Some(card) = discard_pile.pop_back() {
                        player1.push_back(card);
                    }
                }
            }
        }

        if turns < TURN_CUTOFF {
            continue 'game_loop;
        } else {
            suspicious = true;
            println!("!!!!!!!!!!!!!!!!!! SUSPICIOUS GAME");
            break 'game_loop;
        }
    }

    (turns, suspicious)
}

fn main() -> IOResult<()> {
    let mut args = env::args();
    let _ = args.next();
    let threads_to_use = args.next().unwrap().parse::<usize>().unwrap();

    if threads_to_use == 0 {
        println!("At least one thread must be used.");
        return Ok(());
    }

    println!("Loading previous results...");

    let previous_results = fs::read_to_string(CACHE_FILE)?;
    let mut splitted_results = previous_results.split_whitespace();

    let mut actual_most_turns = splitted_results
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_default();

    let mut actual_least_turns = splitted_results
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(usize::MAX);

    let mut actual_suspicious_games = splitted_results
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_default();

    let mut actual_most_hash = splitted_results
        .next()
        .map_or_else(|| String::new(), |s| s.to_owned());

    let mut actual_least_hash = splitted_results
        .next()
        .map_or_else(|| String::new(), |s| s.to_owned());

    println!("Loading permutations to test...");

    let file = File::open(PERMUTATION_FILE)?;

    for line in BufReader::new(file).lines() {
        let line = line?;

        if line.len() > 0 {
            unsafe { HASHES_TO_TEST.push(line) };
        }
    }

    let hash_count = unsafe { HASHES_TO_TEST.len() };

    if hash_count % threads_to_use != 0 {
        println!(
            "The thread count must be a multiple of the hashes to test ({}).",
            hash_count
        );

        return Ok(());
    }

    let hashes_per_thread = hash_count / threads_to_use;

    println!("\nTesting {} hashes in total.", hash_count);
    println!(
        "Starting {} threads to test {} hashes each...",
        threads_to_use, hashes_per_thread
    );

    let (sender, receiver) = channel::unbounded();

    thread::scope(|s| {
        for i in 0..threads_to_use {
            let thread_sender = sender.clone();

            s.spawn(move |_| {
                let mut most_turns = 0;
                let mut least_turns = usize::MAX;

                let mut most_hash = String::with_capacity(DECK_SIZE);
                let mut least_hash = String::with_capacity(DECK_SIZE);

                let mut suspicious_games = 0;

                let mut player1: VecDeque<Card> = VecDeque::with_capacity(DECK_SIZE);
                let mut player2: VecDeque<Card> = VecDeque::with_capacity(DECK_SIZE);
                let mut discard_pile: VecDeque<Card> = VecDeque::with_capacity(DECK_SIZE);

                let mut progress = 0;
                let progress_percent = 100.0 / hashes_per_thread as f64;

                for hash in unsafe {
                    &HASHES_TO_TEST[(i * hashes_per_thread)..((i + 1) * hashes_per_thread)]
                } {
                    progress += 1;

                    get_players_from_hash(&hash, &mut player1, &mut player2);
                    let (turns, suspicious) =
                        simulate_game(&mut player1, &mut player2, &mut discard_pile);

                    if turns > most_turns {
                        most_turns = turns;

                        most_hash.clear();
                        most_hash.push_str(hash);
                    } else if turns < least_turns {
                        least_turns = turns;

                        least_hash.clear();
                        least_hash.push_str(hash);
                    }

                    if suspicious {
                        suspicious_games += 1;
                    }

                    player1.clear();
                    player2.clear();
                    discard_pile.clear();

                    println!(
                        "{} TESTED [{}] - {:.2}%\n",
                        i + 1,
                        hash,
                        progress as f64 * progress_percent
                    );
                }

                thread_sender
                    .send(ThreadData {
                        most_turns,
                        least_turns,
                        suspicious_games,
                        most_hash,
                        least_hash,
                    })
                    .unwrap();
                drop(thread_sender);
            });
        }
    })
    .unwrap();

    drop(sender);
    println!("Collecting and analyzing thread results...");

    for data in receiver.iter() {
        let most_turns = data.most_turns;
        let least_turns = data.least_turns;

        if most_turns > actual_most_turns {
            actual_most_turns = most_turns;
            actual_most_hash = data.most_hash;
        }

        if least_turns < actual_least_turns {
            actual_least_turns = least_turns;
            actual_least_hash = data.least_hash;
        }

        actual_suspicious_games += data.suspicious_games;
    }

    println!(
        "Most Turns:  {:10} turns [{}]",
        actual_most_turns, actual_most_hash
    );
    println!(
        "Least Turns: {:10} turns [{}]",
        actual_least_turns, actual_least_hash
    );
    println!("Suspicious Games: {:5}", actual_suspicious_games);
    println!("\nSaving results...");

    fs::write(
        CACHE_FILE,
        format!(
            "{} {} {} {} {}",
            actual_most_turns,
            actual_least_turns,
            actual_suspicious_games,
            actual_most_hash,
            actual_least_hash
        ),
    )?;

    fs::remove_file(PERMUTATION_FILE)?;

    println!("Done.");

    Ok(())
}
