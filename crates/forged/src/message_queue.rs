use crate::prisma::{self, PrismaClient};
use forge::{ActivityObject, Event};
use crate::Result;
use deadpool_lapin::lapin::message::Delivery;
use deadpool_lapin::lapin::Channel;
use tracing::debug;

pub async fn handle_message(
    deliver: Delivery,
    database: &PrismaClient,
    _job_channel: &Channel,
    _job_inbox_name: &str,
) -> Result<()> {
    let body = deliver.data;
    let envelope: Event = serde_json::from_slice(&body)?;
    match envelope {
        Event::Create(envelope) => {
            debug!("got create event: {:?}", envelope);
            match envelope.object {
                ActivityObject::ChangeRequest(change_request) => {
                    
                    Ok(())
                }
            }
        }
        Event::Update(envelope) => {
            debug!("got update event: {:?}", envelope);
            match envelope.object {
                ActivityObject::ChangeRequest(change_request) => todo!(),
            }
        }
        Event::Delete(envelope) => {
            debug!("got delete event: {:?}", envelope);
            match envelope.object {
                ActivityObject::ChangeRequest(change_request) => todo!(),
            }
        }
    }
}
