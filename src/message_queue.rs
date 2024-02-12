use crate::prisma::PrismaClient;
use crate::{ActivityObject, Event, Result};
use deadpool_lapin::lapin::message::Delivery;
use deadpool_lapin::lapin::Channel;
use tracing::debug;

pub async fn handle_message(
    deliver: Delivery,
    _database: &PrismaClient,
    _job_channel: &Channel,
    _job_inbox_name: &str,
) -> Result<()> {
    let body = deliver.data;
    let envelope: Event = serde_json::from_slice(&body)?;
    match envelope {
        Event::Create(envelope) => {
            debug!("got create event: {:?}", envelope);
            match envelope.object {
                ActivityObject::ChangeRequest(_) => todo!(),
                ActivityObject::JobReport(_) => todo!(),
            }
        }
        Event::Update(envelope) => {
            debug!("got update event: {:?}", envelope);
            match envelope.object {
                ActivityObject::ChangeRequest(_) => todo!(),
                ActivityObject::JobReport(_) => todo!(),
            }
        }
        Event::Delete(envelope) => {
            debug!("got delete event: {:?}", envelope);
            match envelope.object {
                ActivityObject::ChangeRequest(_) => todo!(),
                ActivityObject::JobReport(_) => todo!(),
            }
        }
    }
}
