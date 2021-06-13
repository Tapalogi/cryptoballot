use crate::*;
use cryptid::elgamal::Ciphertext;
use cryptid::threshold::DecryptShare;
use cryptid::threshold::Threshold;
use ed25519_dalek::PublicKey;
use std::collections::HashMap;
use uuid::Uuid;

/// Transaction 8: Partial Decryption
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PartialDecryptionTransaction {
    pub id: Identifier,
    pub election_id: Identifier,

    /// The upstream transaction ID, either the vote transaction ID or the mix transaction ID
    pub upstream_id: Identifier,

    /// If this is from a mix, the index of the ciphertext in the `reencryption` field, or `0` if from a vote transaction
    pub upstream_index: usize,

    pub trustee_id: Uuid,

    #[serde(with = "EdPublicKeyHex")]
    pub trustee_public_key: PublicKey,

    pub partial_decryption: DecryptShare,
}

impl PartialDecryptionTransaction {
    /// Create a new DecryptionTransaction with the decrypted vote
    pub fn new(
        election_id: Identifier,
        upstream_id: Identifier,
        upstream_index: usize,
        trustee_id: Uuid,
        trustee_index: u8,
        trustee_public_key: PublicKey,
        partial_decryption: DecryptShare,
    ) -> Self {
        PartialDecryptionTransaction {
            id: PartialDecryptionTransaction::build_id(election_id, upstream_id, trustee_index),
            election_id,
            upstream_id,
            upstream_index,
            trustee_id,
            trustee_public_key,
            partial_decryption,
        }
    }

    // Has an ID format of <election-id><type><upstream-tx-type><voter-anonymous-key/mix-unique-info><trustee-index>
    pub fn build_id(
        election_id: Identifier,
        upstream_id: Identifier,
        trustee_index: u8,
    ) -> Identifier {
        let mut unique_info = [0; 16];
        unique_info[0] = upstream_id.transaction_type.into();
        unique_info[1..15].copy_from_slice(&upstream_id.unique_id.unwrap()[..14]);
        unique_info[15] = trustee_index;

        Identifier::new(
            election_id,
            TransactionType::PartialDecryption,
            &unique_info,
        )
    }
}

impl Signable for PartialDecryptionTransaction {
    fn id(&self) -> Identifier {
        self.id
    }

    fn public(&self) -> Option<PublicKey> {
        Some(self.trustee_public_key)
    }

    fn inputs(&self) -> Vec<Identifier> {
        vec![self.election_id, self.upstream_id]
    }

    /// Validate the transaction
    fn validate_tx<S: Store>(&self, store: &S) -> Result<(), ValidationError> {
        let election = store.get_election(self.election_id)?;

        // Make sure the trustee is correct
        let mut trustee = None;
        for election_trustee in election.get_full_trustees() {
            if election_trustee.id == self.trustee_id
                && election_trustee.public_key == self.trustee_public_key
            {
                trustee = Some(election_trustee);
                break;
            }
        }
        if trustee.is_none() {
            return Err(ValidationError::TrusteeDoesNotExist(self.trustee_id));
        }

        // Check the ID
        if Self::build_id(self.election_id, self.upstream_id, trustee.unwrap().index) != self.id {
            return Err(ValidationError::IdentifierBadComposition);
        }
        // Make sure the mix index is equal to the minimum number of mixes

        // Make sure voting end exists
        let voting_end_id = Identifier::new(self.election_id, TransactionType::VotingEnd, &[0; 16]);
        if store.get_transaction(voting_end_id).is_none() {
            return Err(ValidationError::MisingVotingEndTransaction);
        }

        // Get the ciphertext either from the vote or the mix
        let ciphertext: Ciphertext = match self.upstream_id.transaction_type {
            TransactionType::Vote => {
                if self.upstream_index != 0 {
                    return Err(ValidationError::InvalidUpstreamIndex);
                }

                store.get_vote(self.upstream_id)?.tx.encrypted_vote
            }
            TransactionType::Mix => {
                let mix = store.get_mix(self.upstream_id)?.tx;

                // Check mix config
                if let Some(mix_config) = election.tx.mixnet {
                    if mix_config.num_shuffles != mix.mix_index {
                        return Err(ValidationError::WrongMixSelected);
                    }
                } else {
                    return Err(ValidationError::InvalidUpstreamID);
                }

                if self.upstream_index >= mix.reencryption.len() {
                    return Err(ValidationError::InvalidUpstreamIndex);
                }

                let mut rencryptions = mix.reencryption;
                rencryptions.swap_remove(self.upstream_index)
            }
            _ => {
                return Err(ValidationError::InvalidUpstreamID);
            }
        };

        // Get the public key transaction for this trustee
        let pkey_tx_id = Identifier::new(
            self.election_id,
            TransactionType::KeyGenPublicKey,
            self.trustee_id.as_bytes(),
        );
        let public_key = store.get_keygen_public_key(pkey_tx_id)?;

        // Validate that the public_key transaction matches
        if self.trustee_id != public_key.inner().trustee_id
            || self.trustee_public_key != public_key.inner().trustee_public_key
        {
            return Err(ValidationError::TrusteePublicKeyMismatch(self.trustee_id));
        }

        // Verify the partial decryption proof
        if !self
            .partial_decryption
            .verify(&public_key.inner().public_key_proof, &ciphertext)
        {
            return Err(ValidationError::PartialDecryptionProofFailed);
        }

        Ok(())
    }
}

/// Transaction 9: Decryption
///
/// After a quorum of Trustees have posted a PartialDecryption transactions, any node may produce
/// a DecryptionTransaction. One DecryptionTransaction is produced for each Vote transaction,
/// decrypting the vote and producing a proof of correct decryption.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DecryptionTransaction {
    pub id: Identifier,
    pub election_id: Identifier,
    pub vote_id: Identifier,
    pub trustees: Vec<Uuid>,

    #[serde(with = "hex_serde")]
    pub decrypted_vote: Vec<u8>,
}

impl DecryptionTransaction {
    /// Create a new DecryptionTransaction with the decrypted vote
    pub fn new(
        election_id: Identifier,
        vote_id: Identifier,
        trustees: Vec<Uuid>,
        decrypted_vote: Vec<u8>,
    ) -> DecryptionTransaction {
        // TODO: sanity check to make sure election and vote are in same election
        // This could be a debug assert
        DecryptionTransaction {
            id: Identifier::new(
                election_id,
                TransactionType::Decryption,
                &vote_id.to_bytes(),
            ),
            election_id,
            vote_id,
            trustees,
            decrypted_vote,
        }
    }
}

impl Signable for DecryptionTransaction {
    fn id(&self) -> Identifier {
        self.id
    }

    // TODO: election authority public key
    fn public(&self) -> Option<PublicKey> {
        None
    }

    fn inputs(&self) -> Vec<Identifier> {
        let mut inputs = Vec::<Identifier>::with_capacity(2 + self.trustees.len());
        inputs.push(self.election_id);
        inputs.push(self.vote_id);

        // TODO: Somehow the partial-decrypt transactions?

        inputs
    }

    /// Validate the transaction
    fn validate_tx<S: Store>(&self, store: &S) -> Result<(), ValidationError> {
        // TODO: Validate ID

        let election = store.get_election(self.election_id)?;

        let voting_end_id = Identifier::new(self.election_id, TransactionType::VotingEnd, &[0; 16]);
        if store.get_transaction(voting_end_id).is_none() {
            return Err(ValidationError::MisingVotingEndTransaction);
        }

        let vote = store.get_vote(self.vote_id)?;

        // Get all pubkeys mapped by trustee ID
        let pubkeys: Vec<KeyGenPublicKeyTransaction> = store
            .get_multiple(self.election_id, TransactionType::KeyGenPublicKey)
            .into_iter()
            .map(|tx| tx.into())
            .map(|tx: Signed<KeyGenPublicKeyTransaction>| tx.tx)
            .collect();

        // Get all partial decryptions mapped by trustee ID
        let mut partials = Vec::with_capacity(self.trustees.len());
        for trustee_id in self.trustees.iter() {
            let trustee = election
                .inner()
                .get_trustee(*trustee_id)
                .ok_or(ValidationError::TrusteeDoesNotExist(*trustee_id))?;
            let partial_id = PartialDecryptionTransaction::build_id(
                self.election_id,
                self.vote_id,
                trustee.index,
            );
            let partial = store.get_partial_decryption(partial_id)?;

            partials.push(partial.tx);
        }

        // Make sure we have enough shares
        let required_shares = election.trustees_threshold as usize;
        if partials.len() < required_shares {
            return Err(ValidationError::NotEnoughShares(
                required_shares,
                partials.len(),
            ));
        }

        // Decrypt the vote
        let decrypted = decrypt_vote(
            &vote.encrypted_vote,
            election.inner().trustees_threshold,
            &election.inner().trustees,
            &pubkeys,
            &partials,
        )?;

        if decrypted != self.decrypted_vote {
            return Err(ValidationError::VoteDecryptionMismatch);
        }

        Ok(())
    }
}

/// Decrypt the vote from the given partial decryptions.
pub fn decrypt_vote(
    encrypted_vote: &Ciphertext,
    trustees_threshold: usize,
    trustees: &[Trustee],
    pubkeys: &[KeyGenPublicKeyTransaction],
    partials: &[PartialDecryptionTransaction],
) -> Result<Vec<u8>, ValidationError> {
    // Map pubkeys by trustee ID
    let pubkeys: HashMap<Uuid, &KeyGenPublicKeyTransaction> =
        pubkeys.into_iter().map(|tx| (tx.trustee_id, tx)).collect();

    // Map partials by trustee ID
    let partials: HashMap<Uuid, &PartialDecryptionTransaction> =
        partials.into_iter().map(|tx| (tx.trustee_id, tx)).collect();

    // Decrypt the vote
    let mut decrypt = cryptid::threshold::Decryption::new(trustees_threshold, encrypted_vote);

    for trustee in trustees {
        if let Some(partial) = partials.get(&trustee.id) {
            if let Some(pubkey) = pubkeys.get(&trustee.id) {
                decrypt.add_share(
                    trustee.index as usize,
                    &pubkey.public_key_proof,
                    &partial.partial_decryption,
                );
            }
        };
    }

    decrypt
        .finish()
        .map_err(|e| ValidationError::VoteDecryptionFailed(e))
}
