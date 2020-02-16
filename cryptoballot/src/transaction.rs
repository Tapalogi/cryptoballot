use crate::*;
use ed25519_dalek::Signature;
use num_enum::TryFromPrimitive;
use rand::Rng;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryInto;
use std::str::FromStr;

pub struct SignedTransaction<T> {
    pub transaction: T,
    pub signature: Signature,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum Transaction {
    Election(ElectionTransaction),
    Vote(VoteTransaction),
    Decryption(DecryptionTransaction),
}

impl Transaction {
    pub fn transaction_type(&self) -> TransactionType {
        // TODO: use a macro
        match self {
            Transaction::Election(_) => TransactionType::Election,
            Transaction::Vote(_) => TransactionType::Vote,
            Transaction::Decryption(_) => TransactionType::Decryption,
        }
    }

    pub fn pack(&self) -> Vec<u8> {
        serde_cbor::to_vec(self).expect("Unexpected error packing transaction")
    }

    pub fn unpack(packed: &[u8]) -> Result<Self, serde_cbor::error::Error> {
        // TODO: translate this error
        serde_cbor::from_slice(packed)
    }

    // TODO: use a macro
    pub fn id(&self) -> TransactionIdentifier {
        match self {
            Transaction::Election(tx) => tx.id,
            Transaction::Vote(tx) => tx.id,
            Transaction::Decryption(tx) => tx.id,
        }
    }
}

// TODO: use a macro
impl From<Transaction> for ElectionTransaction {
    fn from(tx: Transaction) -> Self {
        match tx {
            Transaction::Election(tx) => tx,
            _ => panic!("wrong transaction type expected"),
        }
    }
}

// TODO: use a macro
impl From<Transaction> for VoteTransaction {
    fn from(tx: Transaction) -> Self {
        match tx {
            Transaction::Vote(tx) => tx,
            _ => panic!("wrong transaction type expected"),
        }
    }
}

// TODO: use a macro
impl From<Transaction> for DecryptionTransaction {
    fn from(tx: Transaction) -> Self {
        match tx {
            Transaction::Decryption(tx) => tx,
            _ => panic!("wrong transaction type expected"),
        }
    }
}

// TODO: use a macro
impl From<Transaction> for Option<ElectionTransaction> {
    fn from(tx: Transaction) -> Self {
        match tx {
            Transaction::Election(tx) => Some(tx),
            _ => None,
        }
    }
}

// TODO: use a macro
impl From<Transaction> for Option<VoteTransaction> {
    fn from(tx: Transaction) -> Self {
        match tx {
            Transaction::Vote(tx) => Some(tx),
            _ => None,
        }
    }
}

// TODO: use a macro
impl From<Transaction> for Option<DecryptionTransaction> {
    fn from(tx: Transaction) -> Self {
        match tx {
            Transaction::Decryption(tx) => Some(tx),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, TryFromPrimitive, Copy, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum TransactionType {
    Election,
    Vote,
    Decryption,
}
#[derive(Copy, Clone, PartialEq)]
pub struct TransactionIdentifier {
    pub election_id: [u8; 15],
    pub transaction_type: TransactionType,
    pub unique_id: [u8; 16],
}

impl TransactionIdentifier {
    pub fn new(election_id: TransactionIdentifier, transaction_type: TransactionType) -> Self {
        let mut csprng = rand::rngs::OsRng {};

        let election_id = election_id.election_id;
        let unique_id: [u8; 16] = csprng.gen();
        TransactionIdentifier {
            election_id,
            transaction_type,
            unique_id,
        }
    }

    pub fn new_for_election() -> Self {
        let mut csprng = rand::rngs::OsRng {};

        let election_id: [u8; 15] = csprng.gen();
        let transaction_type = TransactionType::Election;
        let unique_id: [u8; 16] = csprng.gen();
        TransactionIdentifier {
            election_id,
            transaction_type,
            unique_id,
        }
    }
}

impl ToString for TransactionIdentifier {
    fn to_string(&self) -> String {
        let election_id = hex::encode(self.election_id);
        let transaction_type = hex::encode([self.transaction_type as u8]);
        let unique_id = hex::encode(self.unique_id);

        format!("{}{}{}", election_id, transaction_type, unique_id)
    }
}

impl FromStr for TransactionIdentifier {
    type Err = hex::FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s)?;

        // TODO use an error type
        let election_id: [u8; 15] = bytes[0..15].try_into().unwrap();
        let transaction_type = TransactionType::try_from_primitive(bytes[15]).unwrap();
        let unique_id: [u8; 16] = bytes[16..].try_into().unwrap();

        Ok(TransactionIdentifier {
            election_id,
            transaction_type,
            unique_id,
        })
    }
}

impl<'de> Deserialize<'de> for TransactionIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        std::str::FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

impl Serialize for TransactionIdentifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
