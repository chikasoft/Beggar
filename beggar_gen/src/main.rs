use std::env;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Result as IOResult, Write};

const CACHE_FILE: &'static str = "gen_cache.txt";
const PERMUTATION_FILE: &'static str = "permutations.txt";

#[repr(usize)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Card {
    Blank = 0,
    Jack = 1,
    Queen = 2,
    King = 3,
    Ace = 4,
}

fn array_from_hash(hash: &String) -> [Card; 52] {
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

fn hash_from_array(array: &[Card], hash: &mut String) {
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

fn next_permutation(array: &mut [Card]) -> bool {
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

fn main() -> IOResult<()> {
    let mut args = env::args();
    let _ = args.next();
    let num_permutations = args.next().unwrap().parse::<usize>().unwrap();

    let mut hash = match fs::read_to_string(CACHE_FILE) {
        Ok(h) => h,
        Err(_) => "------------------------------------JJJJQQQQKKKKAAAA".to_owned(),
    };

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PERMUTATION_FILE)?;
    let mut writer = BufWriter::new(file);
    let mut array = array_from_hash(&hash);
    let mut actually_generated = 0;

    println!(
        "Trying to generate {} new permutations...",
        num_permutations
    );

    for _ in 0..num_permutations {
        if next_permutation(&mut array) {
            actually_generated += 1;
            hash_from_array(&array, &mut hash);
            writer.write_fmt(format_args!("{}\n", hash))?;
        } else {
            break;
        }
    }

    fs::write(CACHE_FILE, hash)?;

    println!("Done. Generated {} new permutations.", actually_generated);

    Ok(())
}
