//! The service: one entry point, every request.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lpc_cloud_api::request::{
    AddMember, GetEvents, GetHeads, GetProject, HaveBlobs, PublishProject, PushCommit,
    RemoveMember, RevokeSession, SetVisibility, UpdateMe,
};
use lpc_cloud_api::response::{
    Events, Heads, MissingBlobs, ProjectInfo, ProjectList, PushResult, UserInfo,
};
use lpc_cloud_api::{
    Ack, Actor, CloudError, CloudRequest, CloudResponse, DevChoice, DevPickerOptions, HeadInfo,
    LoginOptionsInfo, MeInfo, OidcOption, SessionInfo, SessionList, SidecarMeta, Visibility,
};
use lpc_history::{ContentHash, PrefixedUid, UidPrefix};

use crate::model::caller::Caller;
use crate::model::cloud_project::CloudProject;
use crate::model::cloud_user::CloudUser;
use crate::model::login_providers::LoginProviders;
use crate::model::member_record::MemberRecord;
use crate::model::member_role::MemberRole;
use crate::model::project_refs::ProjectRefs;
use crate::model::session_record::{SESSION_TOKEN_LEN, SessionRecord, session_token_hash};
use crate::ports::clock::Clock;
use crate::ports::id_mint::IdMint;
use crate::ports::meta_store::{MetaStore, normalize_email};
use crate::push_validation::validate_push_events;

/// Longest slug the service will store. Slugs are cosmetic URL decoration;
/// the uid is what identifies a project.
const MAX_SLUG_LEN: usize = 100;

/// Longest given/family name `UpdateMe` will store. Cosmetic, like
/// `MAX_SLUG_LEN` — long enough for any real name, short enough that a
/// client cannot use the field to smuggle a document into the account
/// table.
const MAX_NAME_LEN: usize = 200;

/// How many accounts the dev picker offers at once (`MetaStore::users`'s
/// `limit`). Local dev only, and generous: a seeded-profile picker with
/// hundreds of rows would be a UI problem before it is a query problem.
const DEV_PICKER_CHOICE_LIMIT: usize = 20;

/// The cloud sync service.
///
/// Everything the service can be asked goes through [`handle`](Self::handle);
/// the only other public methods are the two things the auth edge needs and
/// cannot do for itself, because they mint identity: [`upsert_user`](Self::upsert_user)
/// and the session trio ([`open_session`](Self::open_session),
/// [`resolve_session`](Self::resolve_session),
/// [`close_session`](Self::close_session)). Cookies, transport, and the
/// OAuth dance stay at the edge.
///
/// The three type parameters are the ports. `S` is the whole of the
/// service's state; `C` and `I` exist so that no line in this file reads a
/// clock or invents a random number.
pub struct CloudService<S, C, I> {
    store: S,
    clock: C,
    mint: I,
    /// What `LoginOptions` answers from, minus the dev picker's live
    /// choices — see [`with_login_providers`](Self::with_login_providers).
    login_providers: LoginProviders,
}

impl<S: MetaStore, C: Clock, I: IdMint> CloudService<S, C, I> {
    /// Assemble a service from its ports. `LoginOptions` answers with
    /// nothing configured until [`with_login_providers`](Self::with_login_providers)
    /// says otherwise.
    pub fn new(store: S, clock: C, mint: I) -> Self {
        Self {
            store,
            clock,
            mint,
            login_providers: LoginProviders::default(),
        }
    }

    /// Configure the sign-in connections `LoginOptions` reports. Chainable,
    /// so assembly reads as one expression:
    /// `CloudService::new(store, clock, mint).with_login_providers(providers)`.
    /// P3 builds the real value from server config; a service that never
    /// calls this answers an empty `LoginOptions` (no `oidc`, no dev
    /// picker) — a safe default for a test double that has not opted in.
    pub fn with_login_providers(mut self, login_providers: LoginProviders) -> Self {
        self.login_providers = login_providers;
        self
    }

    /// Answer one request on behalf of one caller.
    ///
    /// `caller` carries the already-resolved [`Actor`] (the edge turned a
    /// session cookie into it via [`resolve_session`](Self::resolve_session))
    /// and, when the edge has one, the hash of the session that
    /// authenticated the call — [`list_sessions`](Self::list_sessions) needs
    /// it to mark the caller's own row `current`. A bare `Actor` converts
    /// via [`Into`] (`session: None`), which is what keeps every call site
    /// that only ever had an `Actor` compiling unchanged. This method never
    /// second-guesses `actor` beyond checking that the account still
    /// exists.
    ///
    /// This match is the *only* place in the service that speaks in
    /// `CloudRequest`/`CloudResponse` terms. Every arm hands its payload
    /// straight to a handler that takes the request struct and returns the one
    /// response struct that answers it — the pairing
    /// [`CloudCallSpec`](lpc_cloud_api::CloudCallSpec) declares, enforced here
    /// by the return types rather than by a comment.
    pub fn handle(
        &mut self,
        caller: impl Into<Caller>,
        request: CloudRequest,
    ) -> Result<CloudResponse, CloudError> {
        let caller: Caller = caller.into();
        let actor = caller.actor;
        match request {
            CloudRequest::WhoAmI => self.who_am_i(actor).map(Into::into),
            CloudRequest::ListMyProjects => self.list_my_projects(actor).map(Into::into),
            CloudRequest::PublishProject(request) => {
                self.publish_project(actor, request).map(Into::into)
            }
            CloudRequest::SetVisibility(request) => {
                self.set_visibility(actor, request).map(Into::into)
            }
            CloudRequest::AddMember(request) => self.add_member(actor, request).map(Into::into),
            CloudRequest::RemoveMember(request) => {
                self.remove_member(actor, request).map(Into::into)
            }
            CloudRequest::GetProject(request) => self.get_project(actor, request).map(Into::into),
            CloudRequest::GetHeads(request) => self.get_heads(actor, request).map(Into::into),
            CloudRequest::HaveBlobs(request) => self.have_blobs(actor, request).map(Into::into),
            CloudRequest::PushCommit(request) => self.push_commit(actor, request).map(Into::into),
            CloudRequest::GetEvents(request) => self.get_events(actor, request).map(Into::into),
            CloudRequest::GetMe => self.get_me(caller).map(Into::into),
            CloudRequest::UpdateMe(request) => self.update_me(caller, request).map(Into::into),
            CloudRequest::ListSessions => self.list_sessions(caller).map(Into::into),
            CloudRequest::RevokeSession(request) => {
                self.revoke_session(caller, request).map(Into::into)
            }
            CloudRequest::LoginOptions => self.login_options().map(Into::into),
        }
    }

    /// Record a login, minting the account on first sight.
    ///
    /// Identity is the Google subject, not the email: a person who changes
    /// their address is the same account, and their `email` here is simply
    /// updated. Every call also resolves pending membership rows for the
    /// email (Q4) — that is the moment an invitation becomes access.
    ///
    /// `provider` (`"google"` | `"dev"` today) is only ever set on the
    /// *creation* branch below — an account's sign-in method is fixed at
    /// birth, and a returning login must not silently reassign it via
    /// `..existing`.
    ///
    /// `given_name`/`family_name`/`picture_url` are whatever the edge
    /// captured from the provider's profile this login (`None` is an honest
    /// "the provider told us nothing"). What happens with them is the
    /// Q4/Q5 ruling, straight from [`CloudUser`]'s struct-level seeding
    /// rules: on **creation** they are seeded verbatim and `display_name` is
    /// recomputed from them through the one shared derivation
    /// ([`CloudUser::recompute_display_name`]); on a **returning** login only
    /// `picture_url` is refreshed — `given_name`/`family_name`/
    /// `display_name` are left exactly as they were, because a provider
    /// re-reporting its own idea of a name must not clobber an edit the
    /// account holder made via `UpdateMe`. Edits to this rule stay on
    /// LightPlayer; a fork changing it should say so.
    pub fn upsert_user(
        &mut self,
        google_sub: &str,
        email: &str,
        display_name: &str,
        provider: &str,
        given_name: Option<&str>,
        family_name: Option<&str>,
        picture_url: Option<&str>,
    ) -> CloudUser {
        let email = normalize_email(email);
        let user = match self.store.user_by_google_sub(google_sub) {
            Some(existing) => CloudUser {
                email: email.clone(),
                picture_url: picture_url.map(ToString::to_string),
                ..existing
            },
            None => {
                let mut user = CloudUser {
                    uid: PrefixedUid::mint(UidPrefix::User, &self.mint.uid_bytes()),
                    google_sub: google_sub.into(),
                    email: email.clone(),
                    display_name: display_name.into(),
                    given_name: given_name.map(ToString::to_string),
                    family_name: family_name.map(ToString::to_string),
                    picture_url: picture_url.map(ToString::to_string),
                    provider: provider.into(),
                    created_at: self.clock.now(),
                };
                user.recompute_display_name();
                user
            }
        };
        self.store.put_user(user.clone());
        self.store.resolve_pending_members(&email, user.uid);
        user
    }

    /// Mint a session for an account and return its **raw** token.
    ///
    /// The raw bytes are returned exactly once, for the edge to put in a
    /// cookie; only their hash is stored. `user_agent` is whatever the edge
    /// captured from the request that logged in (P3's job to fill in; `None`
    /// is a legitimate "no header sent", not a placeholder).
    pub fn open_session(
        &mut self,
        user: PrefixedUid,
        ttl_seconds: f64,
        user_agent: Option<String>,
    ) -> [u8; SESSION_TOKEN_LEN] {
        let token = self.mint.session_token();
        let now = self.clock.now();
        self.store.put_session(SessionRecord {
            token_hash: session_token_hash(&token),
            user,
            created_at: now,
            expires_at: now + ttl_seconds,
            user_agent,
        });
        token
    }

    /// Resolve a raw session token to a caller. An unknown or expired token
    /// is simply [`Actor::Anonymous`] — not an error; plenty of requests are
    /// answerable without a session.
    pub fn resolve_session(&self, token: &[u8]) -> Actor {
        match self.store.session(session_token_hash(token)) {
            Some(session) if session.expires_at > self.clock.now() => Actor::User(session.user),
            _ => Actor::Anonymous,
        }
    }

    /// End a session (logout).
    pub fn close_session(&mut self, token: &[u8]) {
        self.store.delete_session(session_token_hash(token));
    }

    /// The service's state, for an edge that needs to read or seed it
    /// (recording a blob after an upload, for instance).
    pub fn store(&self) -> &S {
        &self.store
    }

    /// The service's state, mutably. See [`store`](Self::store).
    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    /// The injected clock.
    pub fn clock(&self) -> &C {
        &self.clock
    }

    // ---- per-request handlers ----------------------------------------
    //
    // One per `CloudRequest` variant, each taking that request's struct (the
    // two payload-free requests take nothing) and returning the response
    // struct that answers it. Wrapping back into `CloudResponse` is `handle`'s
    // job and happens nowhere else.

    fn who_am_i(&self, actor: Actor) -> Result<UserInfo, CloudError> {
        Ok(UserInfo { actor })
    }

    /// Projects the caller is a resolved member of. An anonymous caller is a
    /// member of nothing, which is an answer rather than an error — the
    /// homepage asks this before it knows whether anyone is logged in.
    fn list_my_projects(&self, actor: Actor) -> Result<ProjectList, CloudError> {
        let projects = match actor {
            Actor::Anonymous => Vec::new(),
            Actor::User(uid) => self
                .store
                .projects_for_user(uid)
                .iter()
                .map(CloudProject::to_meta)
                .collect(),
        };
        Ok(ProjectList { projects })
    }

    /// Publish a project the client already minted a uid for (D21).
    ///
    /// Re-publishing your own project restates its slug and visibility.
    /// Publishing a uid someone *else* owns answers `NotFound` — the same
    /// answer as a uid that was never published, so the endpoint cannot be
    /// used to probe which project uids exist.
    fn publish_project(
        &mut self,
        actor: Actor,
        PublishProject {
            uid,
            visibility,
            slug,
        }: PublishProject,
    ) -> Result<ProjectInfo, CloudError> {
        let user = self.require_user(actor)?;
        if uid.prefix() != UidPrefix::Project {
            return Err(invalid("project uid must be a prj uid"));
        }
        validate_slug(&slug)?;

        if let Some(existing) = self.store.project(uid) {
            if existing.owner != user.uid {
                return Err(CloudError::NotFound);
            }
            self.store.put_project(CloudProject {
                visibility,
                slug,
                ..existing
            });
            return self.project_info(uid);
        }

        let now = self.clock.now();
        self.store.put_project(CloudProject {
            uid,
            owner: user.uid,
            visibility,
            slug: slug.clone(),
            created_at: now,
        });
        self.store.put_member(MemberRecord {
            project: uid,
            email: user.email.clone(),
            user: Some(user.uid),
            role: MemberRole::Owner,
            added_at: now,
        });
        self.store.put_refs(uid, ProjectRefs::new());
        // A project with no commits still needs display metadata; the first
        // push replaces this with the client's own.
        self.store.put_sidecar(uid, placeholder_sidecar(&slug));
        self.project_info(uid)
    }

    fn set_visibility(
        &mut self,
        actor: Actor,
        SetVisibility { uid, visibility }: SetVisibility,
    ) -> Result<ProjectInfo, CloudError> {
        let (project, _) = self.require_write_access(actor, uid)?;
        self.store.put_project(CloudProject {
            visibility,
            ..project
        });
        self.project_info(uid)
    }

    /// Grant access by email — to an account that may not exist yet (Q4).
    /// The row is stored unresolved and grants nothing until that email
    /// logs in.
    fn add_member(
        &mut self,
        actor: Actor,
        AddMember { uid, email }: AddMember,
    ) -> Result<ProjectInfo, CloudError> {
        self.require_write_access(actor, uid)?;
        let email = validate_email(&email)?;
        let invited = self.store.user_by_email(&email).map(|user| user.uid);
        let existing_role = self
            .store
            .members(uid)
            .into_iter()
            .find(|member| member.email == email)
            .map(|member| member.role);
        self.store.put_member(MemberRecord {
            project: uid,
            email,
            user: invited,
            // Re-adding the owner must not demote them.
            role: existing_role.unwrap_or(MemberRole::Member),
            added_at: self.clock.now(),
        });
        self.project_info(uid)
    }

    fn remove_member(
        &mut self,
        actor: Actor,
        RemoveMember { uid, email }: RemoveMember,
    ) -> Result<ProjectInfo, CloudError> {
        self.require_write_access(actor, uid)?;
        let email = validate_email(&email)?;
        let owns_the_row = self
            .store
            .members(uid)
            .into_iter()
            .any(|member| member.email == email && member.role == MemberRole::Owner);
        if owns_the_row {
            return Err(invalid("the owner cannot be removed from their project"));
        }
        self.store.remove_member(uid, &email);
        self.project_info(uid)
    }

    fn get_project(
        &self,
        actor: Actor,
        GetProject { uid }: GetProject,
    ) -> Result<ProjectInfo, CloudError> {
        self.require_read_access(actor, uid)?;
        self.project_info(uid)
    }

    fn get_heads(&self, actor: Actor, GetHeads { uid }: GetHeads) -> Result<Heads, CloudError> {
        self.require_read_access(actor, uid)?;
        Ok(Heads {
            heads: self.store.refs(uid).to_head_infos(),
        })
    }

    /// Which of these hashes the service does not have. Authenticated: this
    /// is the pre-flight of a push, and only a member can push.
    fn have_blobs(
        &self,
        actor: Actor,
        HaveBlobs { hashes }: HaveBlobs,
    ) -> Result<MissingBlobs, CloudError> {
        self.require_user(actor)?;
        let mut missing: Vec<ContentHash> = Vec::new();
        for hash in hashes {
            if !self.store.has_blob(hash) && !missing.contains(&hash) {
                missing.push(hash);
            }
        }
        Ok(MissingBlobs { hashes: missing })
    }

    /// Accept a commit.
    ///
    /// The order here is the whole point: everything that can refuse the
    /// push runs before anything is written, and divergence is not on the
    /// list. A push that does not continue the server's line is *recorded*
    /// as a second head (D5) — see [`ProjectRefs::apply_push`] for the
    /// frontier rule and [`crate::push_validation`] for what "well-formed"
    /// means when the events belong to somebody else's line.
    fn push_commit(
        &mut self,
        actor: Actor,
        PushCommit {
            uid,
            parents,
            tree,
            events,
            sidecar,
        }: PushCommit,
    ) -> Result<PushResult, CloudError> {
        self.require_write_access(actor, uid)?;

        // Content-opacity (D3) bounds this check: the server cannot open the
        // tree manifest to enumerate the file blobs it names, so it verifies
        // the hashes it was actually handed. A client that uploads its tree
        // without the files it references breaks only its own pulls.
        let mut missing: Vec<ContentHash> = Vec::new();
        if !self.store.has_blob(tree) {
            missing.push(tree);
        }
        if let Some(preview) = sidecar.preview_png
            && !self.store.has_blob(preview)
        {
            missing.push(preview);
        }
        if !missing.is_empty() {
            return Err(CloudError::MissingBlobs { hashes: missing });
        }

        let stored = self.store.events(uid);
        validate_push_events(&stored, &events)?;

        self.store.append_events(uid, &events);
        let mut refs = self.store.refs(uid);
        let outcome = refs.apply_push(tree, &parents);
        let heads: Vec<HeadInfo> = refs.to_head_infos();
        self.store.put_refs(uid, refs);
        self.store.put_sidecar(uid, sidecar);

        Ok(PushResult { outcome, heads })
    }

    /// Read the log forward. `next_since` is the last sequence number handed
    /// out, so passing it back reads on with no gap and no overlap — and
    /// stays put when there was nothing new.
    fn get_events(
        &self,
        actor: Actor,
        GetEvents { uid, since }: GetEvents,
    ) -> Result<Events, CloudError> {
        self.require_read_access(actor, uid)?;
        let stored = self.store.events_since(uid, since);
        let next_since = stored.last().map(|entry| entry.seq).unwrap_or(since);
        Ok(Events {
            events: stored.into_iter().map(|entry| entry.event).collect(),
            next_since,
        })
    }

    // ---- account / sessions -------------------------------------------

    /// The caller's own account record.
    fn get_me(&self, caller: Caller) -> Result<MeInfo, CloudError> {
        let user = self.require_user(caller.actor)?;
        Ok(self.me_info(user))
    }

    /// Edit the caller's own given/family name.
    ///
    /// Each field is trimmed; empty-after-trim becomes `None` (clearing the
    /// field), and a trimmed value over [`MAX_NAME_LEN`] is refused rather
    /// than silently truncated — a client that hits the limit should know.
    /// `display_name` is recomputed from the result before storing, through
    /// the one shared derivation ([`CloudUser::recompute_display_name`]).
    fn update_me(
        &mut self,
        caller: Caller,
        UpdateMe {
            given_name,
            family_name,
        }: UpdateMe,
    ) -> Result<MeInfo, CloudError> {
        let mut user = self.require_user(caller.actor)?;
        user.given_name = normalize_name(given_name)?;
        user.family_name = normalize_name(family_name)?;
        user.recompute_display_name();
        self.store.put_user(user.clone());
        Ok(self.me_info(user))
    }

    /// Every session open on the caller's own account, newest first, with
    /// the session that made this call marked `current`.
    fn list_sessions(&self, caller: Caller) -> Result<SessionList, CloudError> {
        let user = self.require_user(caller.actor)?;
        let mut sessions: Vec<SessionInfo> = self
            .store
            .sessions_for_user(user.uid)
            .into_iter()
            .map(|session| SessionInfo {
                id: session.token_hash.to_string(),
                created_at: session.created_at,
                expires_at: session.expires_at,
                user_agent: session.user_agent,
                current: caller.session == Some(session.token_hash),
            })
            .collect();
        sessions.sort_by(|a, b| {
            b.created_at
                .partial_cmp(&a.created_at)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        Ok(SessionList { sessions })
    }

    /// End one of the caller's own sessions. A malformed id is a
    /// bad-request refusal; an id that is not hex-shaped, not a known
    /// session, or names a session belonging to someone else, is answered
    /// `NotFound` — the caller cannot use this call to probe which session
    /// ids exist on other accounts.
    fn revoke_session(
        &mut self,
        caller: Caller,
        RevokeSession { id }: RevokeSession,
    ) -> Result<Ack, CloudError> {
        let user = self.require_user(caller.actor)?;
        let hash: ContentHash = id.parse().map_err(|_| invalid("session id must be hex"))?;
        let owned = self
            .store
            .sessions_for_user(user.uid)
            .into_iter()
            .any(|session| session.token_hash == hash);
        if !owned {
            return Err(CloudError::NotFound);
        }
        self.store.delete_session(hash);
        Ok(Ack)
    }

    /// What ways there are to sign in. Anonymous-callable — this is how a
    /// signed-out client discovers what its "Sign in" affordance should do.
    /// The dev picker's `choices` are queried live from `MetaStore::users`
    /// rather than carried in [`LoginProviders`]: they are today's seeded
    /// accounts, not configuration.
    fn login_options(&self) -> Result<LoginOptionsInfo, CloudError> {
        let oidc = self
            .login_providers
            .oidc
            .iter()
            .map(|connection| OidcOption {
                id: connection.id.clone(),
                label: connection.label.clone(),
                start_path: connection.start_path.clone(),
            })
            .collect();
        let dev_picker = self.login_providers.dev_picker.as_ref().map(|picker| {
            let choices = self
                .store
                .users(DEV_PICKER_CHOICE_LIMIT)
                .into_iter()
                .map(|user| DevChoice {
                    email: user.email,
                    display_name: user.display_name,
                })
                .collect();
            DevPickerOptions {
                start_path: picker.start_path.clone(),
                choices,
            }
        });
        Ok(LoginOptionsInfo { oidc, dev_picker })
    }

    /// [`CloudUser`] → [`MeInfo`]: the one place `provider_label` is derived,
    /// so `GetMe` and `UpdateMe`'s answer never disagree on it.
    fn me_info(&self, user: CloudUser) -> MeInfo {
        MeInfo {
            uid: user.uid,
            email: user.email,
            display_name: user.display_name,
            given_name: user.given_name,
            family_name: user.family_name,
            picture_url: user.picture_url,
            provider_label: provider_label(&user.provider),
            created_at: user.created_at,
        }
    }

    // ---- access rules -------------------------------------------------

    /// The caller's account, or `NotAuthenticated`.
    ///
    /// A session naming an account that no longer exists is treated as no
    /// session at all.
    fn require_user(&self, actor: Actor) -> Result<CloudUser, CloudError> {
        match actor {
            Actor::Anonymous => Err(CloudError::NotAuthenticated),
            Actor::User(uid) => self.store.user(uid).ok_or(CloudError::NotAuthenticated),
        }
    }

    /// Read access: a `Link` project is open to anyone holding its uid,
    /// including anonymous callers; a `Private` one is open to members.
    ///
    /// **A private project the caller cannot see answers `NotFound`, never
    /// `NotAuthorized`.** The uid is the share link, so "this project exists
    /// but is not yours" is itself the leak — it turns the API into an
    /// oracle for which uids are real.
    fn require_read_access(
        &self,
        actor: Actor,
        uid: PrefixedUid,
    ) -> Result<CloudProject, CloudError> {
        let project = self.store.project(uid).ok_or(CloudError::NotFound)?;
        if project.visibility == Visibility::Link || self.is_member(uid, actor) {
            Ok(project)
        } else {
            Err(CloudError::NotFound)
        }
    }

    /// Write access: membership, always. Anonymous callers get
    /// `NotAuthenticated` before existence is even considered.
    ///
    /// An authenticated non-member is told `NotAuthorized` on a `Link`
    /// project (whose existence they could already see) and `NotFound` on a
    /// `Private` one (whose existence they could not) — the read rule's leak
    /// budget, applied to writes.
    fn require_write_access(
        &self,
        actor: Actor,
        uid: PrefixedUid,
    ) -> Result<(CloudProject, CloudUser), CloudError> {
        let user = self.require_user(actor)?;
        let project = self.store.project(uid).ok_or(CloudError::NotFound)?;
        if self.store.member_for_user(uid, user.uid).is_some() {
            Ok((project, user))
        } else if project.visibility == Visibility::Link {
            Err(CloudError::NotAuthorized)
        } else {
            Err(CloudError::NotFound)
        }
    }

    fn is_member(&self, project: PrefixedUid, actor: Actor) -> bool {
        match actor {
            Actor::Anonymous => false,
            Actor::User(uid) => self.store.member_for_user(project, uid).is_some(),
        }
    }

    fn project_info(&self, uid: PrefixedUid) -> Result<ProjectInfo, CloudError> {
        let project = self.store.project(uid).ok_or(CloudError::NotFound)?;
        let sidecar = self
            .store
            .sidecar(uid)
            .unwrap_or_else(|| placeholder_sidecar(&project.slug));
        Ok(ProjectInfo {
            meta: project.to_meta(),
            heads: self.store.refs(uid).to_head_infos(),
            sidecar,
        })
    }
}

/// Display metadata for a project that has never been pushed to. Format
/// version 0 means "no commit has told us yet" — the first push replaces
/// this wholesale.
fn placeholder_sidecar(slug: &str) -> SidecarMeta {
    SidecarMeta {
        name: slug.into(),
        format_version: 0,
        preview_png: None,
    }
}

/// Slugs decorate URLs, so they are kept to characters that survive one
/// unescaped.
fn validate_slug(slug: &str) -> Result<(), CloudError> {
    if slug.is_empty() || slug.len() > MAX_SLUG_LEN {
        return Err(invalid("slug must be between 1 and 100 characters"));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(invalid(
            "slug may contain only ASCII letters, digits, '-' and '_'",
        ));
    }
    Ok(())
}

/// Membership is keyed by email, so the email has to be storable and
/// comparable — not provably deliverable. The check is deliberately shallow.
fn validate_email(email: &str) -> Result<String, CloudError> {
    let normalized = normalize_email(email);
    let well_formed = {
        let mut parts = normalized.split('@');
        matches!(
            (parts.next(), parts.next(), parts.next()),
            (Some(local), Some(domain), None) if !local.is_empty() && !domain.is_empty()
        )
    };
    if well_formed {
        Ok(normalized)
    } else {
        Err(invalid("email must look like name@domain"))
    }
}

fn invalid(detail: &str) -> CloudError {
    CloudError::InvalidRequest {
        detail: detail.into(),
    }
}

/// Normalize an `UpdateMe` name field: trim, empty-after-trim becomes
/// `None` (clearing the field), and a trimmed value over [`MAX_NAME_LEN`]
/// characters is refused.
fn normalize_name(name: Option<String>) -> Result<Option<String>, CloudError> {
    let Some(raw) = name else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else if trimmed.chars().count() > MAX_NAME_LEN {
        Err(invalid("name must be at most 200 characters"))
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

/// [`CloudUser::provider`] → [`crate::model::cloud_user`]'s human label for
/// the wire. The two connections this deployment knows about get their own
/// word; anything else (a future self-host password method, say) gets its
/// raw value capitalized rather than a guess at a proper noun.
fn provider_label(provider: &str) -> String {
    match provider {
        "google" => "Google".to_string(),
        "dev" => "Dev".to_string(),
        other => capitalize(other),
    }
}

/// Uppercase the first character, leave the rest alone. `provider` values
/// are ASCII identifiers today, but this does not assume that.
fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// Tests for this file live in `tests/cloud_service.rs`, not below: they run
// against the real in-memory adapters, and `lp-cloud-store-mem` depends on
// this crate. That dev-dependency cycle resolves for an integration target
// but not for a `#[cfg(test)]` module here, which would compile a second copy
// of the crate the adapters do not implement traits for. The file's own
// header explains it; pure logic with no store is still unit-tested in place
// (`push_validation.rs`, `model/project_refs.rs`).
