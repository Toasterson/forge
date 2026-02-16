use crate::entities::actor;
use crate::repositories::ActorRepository;
use crate::services::OidcService;
use miette::{Context, IntoDiagnostic, Result};
use std::sync::Arc;

/// Auth service handles actor registration, key management, and OIDC validation.
///
/// Token issuance is delegated entirely to the OIDC provider.
/// Forged validates incoming OIDC tokens and manages SSH key registration
/// for CLI-based authentication flows.
#[derive(Clone)]
pub struct AuthService {
    actor_repo: Arc<ActorRepository>,
    oidc: Arc<OidcService>,
    email_from: String,
    smtp_url: Option<String>,
}

impl AuthService {
    pub fn new(
        actor_repo: Arc<ActorRepository>,
        oidc: Arc<OidcService>,
        email_from: String,
        smtp_url: Option<String>,
    ) -> Self {
        Self {
            actor_repo,
            oidc,
            email_from,
            smtp_url,
        }
    }

    /// Authenticate via OIDC token: validate, then create or update the actor.
    pub async fn authenticate_oidc(&self, token: &str) -> Result<actor::Model> {
        let claims = self.oidc.validate_token(token).await?;

        let actor = self
            .actor_repo
            .create_or_update_from_oidc(claims.subject, claims.display_name)
            .await
            .wrap_err("failed to create or update actor from OIDC claims")?;

        Ok(actor)
    }

    /// Register a new actor with an SSH public key.
    ///
    /// 1. Parse the SSH public key
    /// 2. Create an unconfirmed actor
    /// 3. Store the SSH public key
    /// 4. Generate an age-encrypted confirmation challenge
    /// 5. Send via email (if SMTP configured) or return the envelope
    pub async fn register_actor(
        &self,
        display_name: String,
        email: String,
        public_key_str: &str,
        key_id: String,
    ) -> Result<(actor::Model, String)> {
        // 1. Parse SSH public key (validate it's a valid OpenSSH key)
        let ssh_pubkey = ssh_key::PublicKey::from_openssh(public_key_str)
            .into_diagnostic()
            .wrap_err(
                "Failed to parse SSH public key.\n\
                 Ensure the key is in OpenSSH format (e.g. 'ssh-ed25519 AAAA...').\n\
                 Supported algorithms: Ed25519, RSA.",
            )?;

        let algorithm = ssh_pubkey.algorithm().to_string();

        // 2. Generate a random challenge
        let challenge = generate_challenge();

        // 3. Create unconfirmed actor
        let actor = self
            .actor_repo
            .create_unconfirmed(display_name, email.clone(), challenge.clone())
            .await?;

        // 4. Store the SSH key (as the original OpenSSH string)
        self.actor_repo
            .add_key(&actor.id, key_id, algorithm, public_key_str.to_string())
            .await?;

        // 5. Encrypt the challenge with the SSH public key using age
        let encrypted_envelope = encrypt_challenge_for_ssh_key(&challenge, public_key_str)
            .wrap_err(
                "Failed to encrypt confirmation challenge.\n\
                 The SSH key may not be supported for age encryption.\n\
                 Supported key types: Ed25519, RSA.",
            )?;

        // 6. Attempt to send email (best-effort)
        if let Some(ref smtp_url) = self.smtp_url {
            if let Err(e) = self
                .send_confirmation_email(&email, &actor.display_name, &encrypted_envelope, smtp_url)
                .await
            {
                tracing::warn!(
                    error = %e,
                    email = %email,
                    "Failed to send confirmation email — user must use the returned envelope"
                );
            }
        } else {
            tracing::info!("SMTP not configured — confirmation envelope returned in response only");
        }

        Ok((actor, encrypted_envelope))
    }

    /// Confirm a registration by verifying the decrypted challenge.
    pub async fn confirm_registration(
        &self,
        actor_id: &str,
        decrypted_challenge: &str,
    ) -> Result<actor::Model> {
        let actor = self.actor_repo.get_by_id(actor_id).await?.ok_or_else(|| {
            miette::miette!(
                "Actor not found: id={}.\n\
                     The registration may have expired or the actor ID is incorrect.",
                actor_id
            )
        })?;

        if actor.confirmed {
            return Err(miette::miette!(
                "Actor {} is already confirmed.\n\
                 You can authenticate using your OIDC provider.",
                actor_id
            ));
        }

        let expected_challenge = actor.confirmation_challenge.as_deref().ok_or_else(|| {
            miette::miette!(
                "No pending confirmation challenge for actor {}.\n\
                     The registration may have already been confirmed or expired.",
                actor_id
            )
        })?;

        if decrypted_challenge != expected_challenge {
            return Err(miette::miette!(
                "Confirmation challenge does not match.\n\
                 Decrypt the confirmation envelope with your SSH private key and try again.\n\
                 Use: echo '<envelope>' | age -d -i ~/.ssh/id_ed25519"
            ));
        }

        let confirmed = self.actor_repo.confirm_actor(actor_id).await?;
        Ok(confirmed)
    }

    /// Add a new SSH key to an existing (confirmed) actor.
    ///
    /// Requires a proof signature: the caller signs a challenge with an existing key
    /// to prove they own the account.
    pub async fn add_actor_key(
        &self,
        actor_id: &str,
        public_key_str: &str,
        key_id: String,
        proof_signature: &[u8],
        proof_key_id: &str,
    ) -> Result<()> {
        // 1. Look up the existing key used for proof
        let existing_key = self
            .actor_repo
            .get_key(actor_id, proof_key_id)
            .await?
            .ok_or_else(|| {
                miette::miette!(
                    "Proof key '{}' not found for actor {}.\n\
                     Provide a valid key_id of an existing SSH key on your account.",
                    proof_key_id,
                    actor_id
                )
            })?;

        // 2. Verify the proof signature using ed25519-dalek
        verify_proof_signature(&existing_key.public_key, proof_signature, actor_id).wrap_err(
            "Proof signature verification failed.\n\
                 Sign the actor_id with your existing SSH private key to prove account ownership.",
        )?;

        // 3. Parse and store the new key
        let ssh_pubkey = ssh_key::PublicKey::from_openssh(public_key_str)
            .into_diagnostic()
            .wrap_err(
                "Failed to parse the new SSH public key.\n\
                 Ensure the key is in OpenSSH format (e.g. 'ssh-ed25519 AAAA...').",
            )?;

        let algorithm = ssh_pubkey.algorithm().to_string();

        self.actor_repo
            .add_key(actor_id, key_id, algorithm, public_key_str.to_string())
            .await?;

        Ok(())
    }

    async fn send_confirmation_email(
        &self,
        to_email: &str,
        display_name: &str,
        envelope: &str,
        smtp_url: &str,
    ) -> Result<()> {
        use lettre::{
            message::header::ContentType, AsyncSmtpTransport, AsyncTransport, Message,
            Tokio1Executor,
        };

        let email = Message::builder()
            .from(
                self.email_from
                    .parse()
                    .into_diagnostic()
                    .wrap_err("invalid sender email address in configuration")?,
            )
            .to(format!("{} <{}>", display_name, to_email)
                .parse()
                .into_diagnostic()
                .wrap_err("invalid recipient email address")?)
            .subject("Forge Registration Confirmation")
            .header(ContentType::TEXT_PLAIN)
            .body(format!(
                "Welcome to Forge, {}!\n\n\
                 To confirm your registration, decrypt the following envelope \
                 with your SSH private key:\n\n\
                 {}\n\n\
                 Use: echo '<envelope>' | age -d -i ~/.ssh/id_ed25519\n\n\
                 Then submit the decrypted challenge via:\n\
                   pkgdev auth confirm <actor_id> <decrypted_challenge>\n",
                display_name, envelope
            ))
            .into_diagnostic()
            .wrap_err("failed to build confirmation email")?;

        let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(smtp_url)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "Failed to connect to SMTP server at {}.\n\
                     Check the smtp_url in your configuration.",
                    smtp_url
                )
            })?
            .build();

        mailer
            .send(email)
            .await
            .into_diagnostic()
            .wrap_err("failed to send confirmation email")?;

        tracing::info!(to = %to_email, "Sent registration confirmation email");
        Ok(())
    }
}

/// Generate a cryptographically random challenge string.
fn generate_challenge() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    hex::encode(bytes)
}

/// Encrypt a challenge string using age with an SSH public key as recipient.
fn encrypt_challenge_for_ssh_key(challenge: &str, ssh_pubkey_str: &str) -> Result<String> {
    use age::ssh::Recipient;

    let recipient: Recipient = ssh_pubkey_str.parse().map_err(|_| {
        miette::miette!(
            "Failed to parse SSH key as age recipient.\n\
                 Ensure the key is in OpenSSH format: ssh-ed25519 AAAA... or ssh-rsa AAAA..."
        )
    })?;

    let encrypted = age::encrypt(&recipient, challenge.as_bytes())
        .into_diagnostic()
        .wrap_err("Failed to encrypt confirmation challenge with SSH public key.")?;

    use base64::Engine as _;
    Ok(base64::engine::general_purpose::STANDARD.encode(&encrypted))
}

/// Verify a proof signature (Ed25519) over the actor_id.
///
/// The stored public key is in OpenSSH format.
/// We parse it to extract the raw Ed25519 key material for verification.
fn verify_proof_signature(
    stored_public_key: &str,
    signature_bytes: &[u8],
    message: &str,
) -> Result<()> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let ssh_pubkey = ssh_key::PublicKey::from_openssh(stored_public_key)
        .into_diagnostic()
        .wrap_err("Failed to parse stored SSH public key.")?;

    // Extract the raw Ed25519 key bytes
    let ed25519_key = match ssh_pubkey.key_data() {
        ssh_key::public::KeyData::Ed25519(k) => k,
        _ => {
            return Err(miette::miette!(
                "Proof signature verification is only supported for Ed25519 keys.\n\
                 The stored key uses algorithm: {}",
                ssh_pubkey.algorithm()
            ));
        }
    };

    let verifying_key = VerifyingKey::from_bytes(ed25519_key.as_ref())
        .into_diagnostic()
        .wrap_err("Failed to reconstruct Ed25519 verifying key.")?;

    let signature = Signature::from_bytes(signature_bytes.try_into().map_err(|_| {
        miette::miette!(
            "Invalid signature length: expected 64 bytes, got {}.\n\
                     Ensure you are signing with an Ed25519 key.",
            signature_bytes.len()
        )
    })?);

    verifying_key
        .verify(message.as_bytes(), &signature)
        .into_diagnostic()
        .wrap_err(
            "Signature verification failed.\n\
             The proof signature does not match the stored public key.",
        )?;

    Ok(())
}
