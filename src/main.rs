extern crate crossbeam;

use std::collections::VecDeque;
use std::env;
use std::fs;
use std::io::Result as IOResult;

use crossbeam::{channel, thread};

const DECK_SIZE: usize = 52;
const HAND_SIZE: usize = 26;

const TURN_CUTOFF: usize = 10000;

const CACHE_FILE: &'static str = "cache.txt";

static mut HASHES_TO_TEST: Vec<String> = Vec::new();

#[repr(usize)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
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

fn get_array_from_hash(hash: &String) -> [Card; 52] {
    assert_eq!(hash.len(), 52);

    let mut i = 0;
    let mut array = [Card::Blank; 52];

    for c in hash.chars() {
        match c {
            'J' => array[i] = Card::Jack,
            'Q' => array[i] = Card::Queen,
            'K' => array[i] = Card::King,
            'A' => array[i] = Card::Ace,
            _ => {}
        }

        i += 1;
    }

    array
}

fn get_hash_from_array(array: &[Card], hash: &mut String) {
    hash.clear();

    for card in array {
        hash.push(match card {
            Card::Blank => '-',
            Card::Jack => 'J',
            Card::Queen => 'Q',
            Card::King => 'K',
            Card::Ace => 'A',
        });
    }
}

fn get_players_from_hash(hash: &str, player1: &mut VecDeque<Card>, player2: &mut VecDeque<Card>) {
    for c in hash.chars().take(HAND_SIZE) {
        player1.push_back(Card::from_char(c));
    }

    for c in hash.chars().skip(HAND_SIZE).take(HAND_SIZE) {
        player2.push_back(Card::from_char(c));
    }
}

fn has_next_permutation(array: &mut [Card]) -> bool {
    if array.is_empty() {
        return false;
    }

    let mut i = array.len() - 1;

    while i > 0 && array[i - 1] >= array[i] {
        i -= 1;
    }

    if i == 0 {
        return false;
    }

    let mut j = array.len() - 1;

    while array[j] <= array[i - 1] {
        j -= 1;
    }

    array.swap(i - 1, j);
    array[i..].reverse();

    true
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
            break 'game_loop;
        }
    }

    (turns, suspicious)
}

fn main() -> IOResult<()> {
    let mut args = env::args();
    let _ = args.next();
    let games_to_test = args.next().unwrap().parse::<usize>().unwrap();
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

    let mut actual_most_hash = splitted_results
        .next()
        .map_or_else(|| String::new(), |s| s.to_owned());

    let mut actual_least_hash = splitted_results
        .next()
        .map_or_else(|| String::new(), |s| s.to_owned());

    let mut last_tested_hash = splitted_results.next().map_or_else(
        || "------------------------------------JJJJQQQQKKKKAAAA".to_owned(),
        |s| s.to_owned(),
    );

    println!("Generating {} new permutations to test...", games_to_test);
    let mut permutations_generated = 0;
    {
        let permutation_percent = 100.0 / games_to_test as f64;
        let mut tmp_game = get_array_from_hash(&last_tested_hash);

        for _ in 0..games_to_test {
            if has_next_permutation(&mut tmp_game) {
                permutations_generated += 1;
                get_hash_from_array(&tmp_game, &mut last_tested_hash);
                unsafe { HASHES_TO_TEST.push(last_tested_hash.clone()) };
                println!(
                    "[{}] - {:.2}%",
                    last_tested_hash,
                    permutations_generated as f64 * permutation_percent
                );
            } else {
                break;
            }
        }
    }

    println!("Generated {} new permutations.", permutations_generated);

    if permutations_generated % threads_to_use != 0 {
        println!(
            "The thread count must be a multiple of the hashes to test ({}).",
            permutations_generated
        );

        return Ok(());
    }

    let hashes_per_thread = permutations_generated / threads_to_use;

    println!("\nTesting {} hashes in total.", permutations_generated);
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

    let mut actual_suspicious_games = 0;

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

    println!("--- ALL TIME STATS ---");
    println!(
        "Most Turns:  {:10} turns [{}]",
        actual_most_turns, actual_most_hash
    );
    println!(
        "Least Turns: {:10} turns [{}]",
        actual_least_turns, actual_least_hash
    );
    println!();

    println!("--- ONE-OFF STATS ---");
    println!("Suspicious Games: {:5}", actual_suspicious_games);
    println!();

    println!("Saving results...");

    fs::write(
        CACHE_FILE,
        format!(
            "{} {} {} {} {}",
            actual_most_turns,
            actual_least_turns,
            actual_most_hash,
            actual_least_hash,
            last_tested_hash
        ),
    )?;

    println!("Done.");

    Ok(())
}
