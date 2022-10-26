use crate::{
  activities::{
    community::{
      announce::GetCommunity,
      get_community_from_moderators_url,
      send_activity_in_community,
    },
    generate_activity_id,
    verify_add_remove_moderator_target,
    verify_is_public,
    verify_mod_action,
    verify_person_in_community,
  },
  activity_lists::AnnouncableActivities,
  check_apub_id_valid,
  fetch_local_site_data,
  generate_moderators_url,
  local_instance,
  objects::{community::ApubCommunity, person::ApubPerson},
  protocol::activities::community::remove_mod::RemoveMod,
  ActorType,
};
use activitypub_federation::{
  core::object_id::ObjectId,
  data::Data,
  traits::{ActivityHandler, Actor},
};
use activitystreams_kinds::{activity::RemoveType, public};
use lemmy_db_schema::{
  source::{
    community::{CommunityModerator, CommunityModeratorForm},
    moderator::{ModAddCommunity, ModAddCommunityForm},
  },
  traits::{Crud, Joinable},
};
use lemmy_utils::error::LemmyError;
use lemmy_websocket::LemmyContext;
use url::Url;

impl RemoveMod {
  #[tracing::instrument(skip_all)]
  pub async fn send(
    community: &ApubCommunity,
    removed_mod: &ApubPerson,
    actor: &ApubPerson,
    context: &LemmyContext,
  ) -> Result<(), LemmyError> {
    let id = generate_activity_id(
      RemoveType::Remove,
      &context.settings().get_protocol_and_hostname(),
    )?;
    let remove = RemoveMod {
      actor: ObjectId::new(actor.actor_id()),
      to: vec![public()],
      object: ObjectId::new(removed_mod.actor_id()),
      target: generate_moderators_url(&community.actor_id)?.into(),
      id: id.clone(),
      cc: vec![community.actor_id()],
      kind: RemoveType::Remove,
      unparsed: Default::default(),
    };

    let activity = AnnouncableActivities::RemoveMod(remove);
    let inboxes = vec![removed_mod.shared_inbox_or_inbox()];
    send_activity_in_community(activity, actor, community, inboxes, context).await
  }
}

#[async_trait::async_trait(?Send)]
impl ActivityHandler for RemoveMod {
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

    verify_is_public(&self.to, &self.cc)?;
    let community = self.get_community(context, request_counter).await?;
    verify_person_in_community(&self.actor, &community, context, request_counter).await?;
    verify_mod_action(
      &self.actor,
      self.object.inner(),
      community.id,
      context,
      request_counter,
    )
    .await?;
    verify_add_remove_moderator_target(&self.target, &community)?;
    Ok(())
  }

  #[tracing::instrument(skip_all)]
  async fn receive(
    self,
    context: &Data<LemmyContext>,
    request_counter: &mut i32,
  ) -> Result<(), LemmyError> {
    let community = self.get_community(context, request_counter).await?;
    let remove_mod = self
      .object
      .dereference(context, local_instance(context).await, request_counter)
      .await?;

    let form = CommunityModeratorForm {
      community_id: community.id,
      person_id: remove_mod.id,
    };
    CommunityModerator::leave(context.pool(), &form).await?;

    // write mod log
    let actor = self
      .actor
      .dereference(context, local_instance(context).await, request_counter)
      .await?;
    let form = ModAddCommunityForm {
      mod_person_id: actor.id,
      other_person_id: remove_mod.id,
      community_id: community.id,
      removed: Some(true),
    };
    ModAddCommunity::create(context.pool(), &form).await?;

    // TODO: send websocket notification about removed mod
    Ok(())
  }
}

#[async_trait::async_trait(?Send)]
impl GetCommunity for RemoveMod {
  #[tracing::instrument(skip_all)]
  async fn get_community(
    &self,
    context: &LemmyContext,
    request_counter: &mut i32,
  ) -> Result<ApubCommunity, LemmyError> {
    get_community_from_moderators_url(&self.target, context, request_counter).await
  }
}
