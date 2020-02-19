use super::*;
use uuid::Uuid;

#[test]
fn end_to_end_election() {
    // Create election authority public and private key
    let (authority_secret, authority_public) = generate_keypair();

    // Create a ballot (TODO: make this a proper struct)
    let ballot_id = Uuid::new_v4();

    // Create an authenticator
    let (authenticator, authn_secrets) = Authenticator::new(256, &vec![ballot_id]).unwrap();
    let authn_secret = authn_secrets.get(&ballot_id).unwrap();
    let authn_public = authenticator.public_keys.get(&ballot_id).unwrap().as_ref();

    // Create 3 trustees
    let (trustee_1, trustee_1_secret) = Trustee::new();
    let (trustee_2, trustee_2_secret) = Trustee::new();
    let (trustee_3, _trustee_3_secret) = Trustee::new();

    // Create an election transaction with a single ballot
    let (mut election, election_secret) = ElectionTransaction::new(authority_public);
    election.ballots = vec![ballot_id];
    election.authenticators = vec![authenticator.clone()];
    election.trustees = vec![trustee_1.clone(), trustee_2.clone(), trustee_3.clone()];
    election.trustees_threshold = 2;

    // Finalize election transaction by signing it
    let election = Signed::sign(&authority_secret, election).unwrap();

    // Deal the secret shares to the trustees
    let mut shares = deal_secret_shares(
        election.trustees_threshold,
        election.trustees.len(),
        &election_secret.serialize(),
    );
    let trustee_1_share = shares.pop().unwrap();
    let trustee_2_share = shares.pop().unwrap();

    // TODO: In the future, don't rely on a trusted dealer, instead do verifiable distributed key generation using ElGamal

    // Validate the election transaction
    election.verify_signature().unwrap();
    election.validate().unwrap();

    // Generate an empty vote transaction
    let (mut vote, voter_secret) = VoteTransaction::new(election.id(), ballot_id);

    // Create an auth package and blind it
    let auth_package = AuthPackage::new(election.id(), ballot_id, vote.anonymous_key);
    let (blinded_auth_package, unblinder) = auth_package.blind(&authn_public);

    // Authenticate the voter (for a real election the voter would pass additional auth info)
    let authentication = authenticator.authenticate(&authn_secret, &blinded_auth_package);
    let authentication = authentication.unblind(&authn_public, unblinder);
    vote.authentication.push(authentication);

    // Create a vote transaction
    let secret_vote = "Barak Obama";

    // Encrypt the secret vote
    vote.encrypted_vote =
        encrypt_vote(&election.encryption_public, secret_vote.as_bytes()).unwrap();

    // Sign and seal the vote transaction
    let vote = Signed::sign(&voter_secret, vote).unwrap();

    // Validate the vote transaction
    vote.verify_signature().unwrap();
    vote.validate(&election).unwrap();

    // Voting is over
    // ----------------

    // Create SecretShare transactions - only 2 of 3!
    let secret_share_1 = SecretShareTransaction::new(election.id(), trustee_1, trustee_1_share);
    let secret_share_2 = SecretShareTransaction::new(election.id(), trustee_2, trustee_2_share);

    // Sign and seal Secretshare transactions
    let secret_share_1 = Signed::sign(&trustee_1_secret, secret_share_1).unwrap();
    let secret_share_2 = Signed::sign(&trustee_2_secret, secret_share_2).unwrap();

    // Validate SecretShare transactions
    secret_share_1.verify_signature().unwrap();
    secret_share_1.validate(&election).unwrap();
    secret_share_2.verify_signature().unwrap();
    secret_share_2.validate(&election).unwrap();

    // Sign the secret-share transaction

    // Recover election key from two trustees
    let shares = vec![
        secret_share_1.secret_share.clone(),
        secret_share_2.secret_share.clone(),
    ];
    let election_key = recover_secret_from_shares(election.trustees_threshold, shares).unwrap();

    // Decrypt the votes
    let decrypted_vote = decrypt_vote(&election_key, &vote.encrypted_vote).unwrap();

    // Create decryption transaction
    let trustees: Vec<Uuid> = election.trustees.iter().map(|t| t.id).collect();
    let decryption = DecryptionTransaction::new(election.id(), vote.id(), trustees, decrypted_vote);
    let decryption = Signed::sign(&authority_secret, decryption).unwrap();

    // Validate decryption transaction
    let secret_share_transactions = vec![
        secret_share_1.inner().to_owned(),
        secret_share_2.inner().to_owned(),
    ];

    // Validate the vote transaction
    decryption.verify_signature().unwrap();
    decryption
        .validate(&election, &vote, &secret_share_transactions)
        .unwrap();

    // To print out the transactions, do `cargo test -- --nocapture`
    println!(
        "{}",
        serde_json::to_string_pretty(&SignedTransaction::from(election)).unwrap()
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&SignedTransaction::from(vote)).unwrap()
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&SignedTransaction::from(secret_share_1)).unwrap()
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&SignedTransaction::from(secret_share_2)).unwrap()
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&SignedTransaction::from(decryption)).unwrap()
    );

    // TODO: tally!
}
