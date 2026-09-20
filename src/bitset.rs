#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BitSet {
    pub(crate) words: Vec<u64>,
}

impl BitSet {
    pub(crate) fn empty(bit_len: usize) -> Self {
        Self {
            words: vec![0; bit_len.div_ceil(64)],
        }
    }

    pub(crate) fn full(bit_len: usize) -> Self {
        let mut result = Self {
            words: vec![u64::MAX; bit_len.div_ceil(64)],
        };
        if let Some(last) = result.words.last_mut() {
            let remainder = bit_len % 64;
            if remainder != 0 {
                *last = (1_u64 << remainder) - 1;
            }
        }
        result
    }

    pub(crate) fn set(&mut self, bit: usize) {
        self.words[bit / 64] |= 1_u64 << (bit % 64);
    }

    pub(crate) fn clear(&mut self, bit: usize) {
        self.words[bit / 64] &= !(1_u64 << (bit % 64));
    }

    pub(crate) fn contains(&self, bit: usize) -> bool {
        self.words[bit / 64] & (1_u64 << (bit % 64)) != 0
    }

    pub(crate) fn count(&self) -> u64 {
        self.words
            .iter()
            .map(|word| u64::from(word.count_ones()))
            .sum()
    }

    pub(crate) fn intersection(&self, other: &Self) -> Self {
        Self {
            words: self
                .words
                .iter()
                .zip(&other.words)
                .map(|(left, right)| left & right)
                .collect(),
        }
    }

    pub(crate) fn intersection_count(&self, other: &Self) -> u64 {
        self.words
            .iter()
            .zip(&other.words)
            .map(|(left, right)| u64::from((left & right).count_ones()))
            .sum()
    }

    pub(crate) fn pop_first(&mut self) -> Option<usize> {
        for (word_index, word) in self.words.iter_mut().enumerate() {
            if *word != 0 {
                let offset = word.trailing_zeros() as usize;
                *word &= *word - 1;
                return Some(64 * word_index + offset);
            }
        }
        None
    }
}
