// Cuely is an open source web search engine.
// Copyright (C) 2022 Cuely ApS
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

// Modified from: https://github.com/DimaKudosh/word2vec
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use byteorder::{LittleEndian, ReadBytesExt};
use flate2::bufread::MultiGzDecoder;
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Wrong header")]
    WrongHeader,

    #[error("IO")]
    IO(#[from] std::io::Error),

    #[error("Bincode")]
    Bincode(#[from] bincode::Error),
}

struct WordVectorReader<R: BufRead> {
    vocabulary_size: usize,
    vector_size: usize,
    reader: R,
    ended_early: bool,
    vectors_read: usize,
}

impl<R: BufRead> WordVectorReader<R> {
    pub fn new_from_reader(mut reader: R) -> Result<WordVectorReader<R>, Error> {
        // Read UTF8 header string from start of file
        let mut header = String::new();
        reader.read_line(&mut header)?;

        //Parse 2 integers, separated by whitespace
        let header_info = header
            .split_whitespace()
            .filter_map(|x| x.parse::<usize>().ok())
            .take(2)
            .collect::<Vec<usize>>();
        if header_info.len() != 2 {
            return Err(Error::WrongHeader);
        }

        //We've successfully read the header, ready to read vectors
        Ok(WordVectorReader {
            vocabulary_size: header_info[0],
            vector_size: header_info[1],
            vectors_read: 0,
            ended_early: false,
            reader,
        })
    }
}

impl<R: BufRead> Iterator for WordVectorReader<R> {
    type Item = (String, Vec<f32>);

    fn next(&mut self) -> Option<(String, Vec<f32>)> {
        if self.vectors_read == self.vocabulary_size {
            return None;
        }

        // Read the bytes of the word string
        let mut word_bytes: Vec<u8> = Vec::new();
        if self.reader.read_until(b' ', &mut word_bytes).is_err() {
            // End the stream if a read error occured
            self.ended_early = true;
            return None;
        }

        // trim newlines, some vector files have newlines in front of a new word, others don't
        let word = match String::from_utf8(word_bytes) {
            Err(_) => {
                self.ended_early = true;
                return None;
            }
            Ok(word) => word.trim().into(),
        };

        // Read floats of the vector
        let mut vector: Vec<f32> = Vec::with_capacity(self.vector_size);
        for _ in 0..self.vector_size {
            match self.reader.read_f32::<LittleEndian>() {
                Err(_) => {
                    self.ended_early = true;
                    return None;
                }
                Ok(value) => vector.push(value),
            }
        }

        self.vectors_read += 1;
        Some((word, vector))
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WordVec(Vec<f32>);
impl WordVec {
    fn magnitude(&self) -> f32 {
        self.0.iter().map(|f| f.powf(2.0)).sum::<f32>().sqrt()
    }

    pub(crate) fn sim(&self, other: &WordVec) -> f32 {
        self.0
            .iter()
            .zip(other.0.iter())
            .map(|(a, b)| a * b)
            .sum::<f32>()
            / (self.magnitude() * other.magnitude())
    }
}

#[derive(Serialize, Deserialize)]
pub struct Word2Vec {
    vectors: HashMap<String, WordVec>,
}

impl Word2Vec {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let reader = BufReader::new(MultiGzDecoder::new(BufReader::new(File::open(path)?)));
        let reader = WordVectorReader::new_from_reader(reader)?;
        let vectors: HashMap<_, _> = reader.map(|(word, vec)| (word, WordVec(vec))).collect();

        Ok(Self { vectors })
    }

    pub(crate) fn get(&self, word: &str) -> Option<&WordVec> {
        self.vectors.get(word)
    }
}
