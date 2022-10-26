use crate::{
  activities::{
    community::{announce::GetCommunity, send_activity_in_community},
    generate_activity_id,
    verify_person_in_community,
    voting::{undo_vote_comment, undo_vote_post},
  },
  activity_lists::AnnouncableActivities,
  check_apub_id_valid,
  fetch_local_site_data,
  local_instance,
  objects::{community::ApubCommunity, person::ApubPerson},
  protocol::activities::voting::{
    undo_vote::UndoVote,
    vote::{Vote, VoteType},
  },
  ActorType,
  PostOrComment,
};
use activitypub_federation::{
  core::object_id::ObjectId,
  data::Data,
  traits::ActivityHandler,
  utils::verify_urls_match,
};
use activitystreams_kinds::activity::UndoType;
use lemmy_db_schema::{newtypes::CommunityId, source::community::Community, traits::Crud};
use lemmy_utils::error::LemmyError;
use lemmy_websocket::LemmyContext;
use url::Url;

impl UndoVote {
  /// UndoVote has as:Public value in cc field, unlike other activities. This indicates to other
  /// software (like GNU social, or presumably Mastodon), that the like actor should not be
  /// disclosed.
  #[tracing::instrument(skip_all)]
  pub async fn send(
    object: &PostOrComment,
    actor: &ApubPerson,
    community_id: CommunityId,
    kind: VoteType,
    context: &LemmyContext,
  ) -> Result<(), LemmyError> {
    let community: ApubCommunity = Community::read(context.pool(), community_id).await?.into();

    let object = Vote::new(object, actor, kind.clone(), context)?;
    let id = generate_activity_id(
      UndoType::Undo,
      &context.settings().get_protocol_and_hostname(),
    )?;
    let undo_vote = UndoVote {
      actor: ObjectId::new(actor.actor_id()),
      object,
      kind: UndoType::Undo,
      id: id.clone(),
      unparsed: Default::default(),
    };
    let activity = AnnouncableActivities::UndoVote(undo_vote);
    send_activity_in_community(activity, actor, &community, vec![], context).await
  }
}

#[async_trait::async_trait(?Send)]
impl ActivityHandler for UndoVote {
  type DataType = LemmyContext;
  type Error = LemmyError;

  fn id(&self) -> &Url {
    &self.id
  }

  fn actor(&self) -> &Url {
    self.actor.inner()
  }

  #[tracing::instrument(skip_all)]
  async fn verify(
    &self,
    context: &Data<LemmyContext>,
    request_counter: &mut i32,
  ) -> Result<(), LemmyError> {
    let local_site_data = fetch_local_site_data(context.pool()).await?;
    check_apub_id_valid(self.id(), &local_site_data, context.settings())
      .map_err(LemmyError::from_message)?;
    let community = self.get_community(context, request_counter).await?;
    verify_person_in_community(&self.actor, &community, context, request_counter).await?;
    verify_urls_match(self.actor.inner(), self.object.actor.inner())?;
    self.object.verify(context, request_counter).await?;
    Ok(())
  }

  #[tracing::instrument(skip_all)]
  async fn receive(
    self,
    context: &Data<LemmyContext>,
    request_counter: &mut i32,
  ) -> Result<(), LemmyError> {
    let actor = self
      .actor
      .dereference(context, local_instance(context).await, request_counter)
      .await?;
    let object = self
      .object
      .object
      .dereference(context, local_instance(context).await, request_counter)
      .await?;
    match object {
      PostOrComment::Post(p) => undo_vote_post(actor, &p, context).await,
      PostOrComment::Comment(c) => undo_vote_comment(actor, &c, context).await,
    }
  }
}

#[async_trait::async_trait(?Send)]
impl GetCommunity for UndoVote {
  #[tracing::instrument(skip_all)]
  async fn get_community(
    &self,
    context: &LemmyContext,
    request_counter: &mut i32,
  ) -> Result<ApubCommunity, LemmyError> {
    self.object.get_community(context, request_counter).await
  }
}
