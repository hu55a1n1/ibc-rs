use crate::prelude::*;

use ibc_proto::google::protobuf::Any;

use crate::core::ics02_client::msgs::{create_client, update_client, upgrade_client, ClientMsg};
use crate::core::ics03_connection::msgs::{
    conn_open_ack, conn_open_confirm, conn_open_init, conn_open_try, ConnectionMsg,
};
use crate::core::ics04_channel::msgs::{
    acknowledgement, chan_close_confirm, chan_close_init, chan_open_ack, chan_open_confirm,
    chan_open_init, chan_open_try, recv_packet, timeout, timeout_on_close, ChannelMsg, PacketMsg,
};
use crate::core::ics26_routing::error::Error;
use ibc_proto::protobuf::Protobuf;

/// Enumeration of all messages that the local ICS26 module is capable of routing.
#[derive(Clone, Debug)]
pub enum Ics26Envelope {
    Ics2Msg(ClientMsg),
    Ics3Msg(ConnectionMsg),
    Ics4ChannelMsg(ChannelMsg),
    Ics4PacketMsg(PacketMsg),
}

impl TryFrom<Any> for Ics26Envelope {
    type Error = Error;

    fn try_from(any_msg: Any) -> Result<Self, Self::Error> {
        match any_msg.type_url.as_str() {
            // ICS2 messages
            create_client::TYPE_URL => {
                // Pop out the message and then wrap it in the corresponding type.
                let domain_msg = create_client::MsgCreateClient::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics2Msg(ClientMsg::CreateClient(domain_msg)))
            }
            update_client::TYPE_URL => {
                let domain_msg = update_client::MsgUpdateClient::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics2Msg(ClientMsg::UpdateClient(domain_msg)))
            }
            upgrade_client::TYPE_URL => {
                let domain_msg = upgrade_client::MsgUpgradeClient::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics2Msg(ClientMsg::UpgradeClient(domain_msg)))
            }

            // ICS03
            conn_open_init::TYPE_URL => {
                let domain_msg = conn_open_init::MsgConnectionOpenInit::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics3Msg(ConnectionMsg::ConnectionOpenInit(
                    domain_msg,
                )))
            }
            conn_open_try::TYPE_URL => {
                let domain_msg = conn_open_try::MsgConnectionOpenTry::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics3Msg(ConnectionMsg::ConnectionOpenTry(
                    Box::new(domain_msg),
                )))
            }
            conn_open_ack::TYPE_URL => {
                let domain_msg = conn_open_ack::MsgConnectionOpenAck::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics3Msg(ConnectionMsg::ConnectionOpenAck(
                    Box::new(domain_msg),
                )))
            }
            conn_open_confirm::TYPE_URL => {
                let domain_msg =
                    conn_open_confirm::MsgConnectionOpenConfirm::decode_vec(&any_msg.value)
                        .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics3Msg(
                    ConnectionMsg::ConnectionOpenConfirm(domain_msg),
                ))
            }

            // ICS04 channel messages
            chan_open_init::TYPE_URL => {
                let domain_msg = chan_open_init::MsgChannelOpenInit::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics4ChannelMsg(ChannelMsg::ChannelOpenInit(
                    domain_msg,
                )))
            }
            chan_open_try::TYPE_URL => {
                let domain_msg = chan_open_try::MsgChannelOpenTry::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics4ChannelMsg(ChannelMsg::ChannelOpenTry(
                    domain_msg,
                )))
            }
            chan_open_ack::TYPE_URL => {
                let domain_msg = chan_open_ack::MsgChannelOpenAck::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics4ChannelMsg(ChannelMsg::ChannelOpenAck(
                    domain_msg,
                )))
            }
            chan_open_confirm::TYPE_URL => {
                let domain_msg =
                    chan_open_confirm::MsgChannelOpenConfirm::decode_vec(&any_msg.value)
                        .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics4ChannelMsg(
                    ChannelMsg::ChannelOpenConfirm(domain_msg),
                ))
            }
            chan_close_init::TYPE_URL => {
                let domain_msg = chan_close_init::MsgChannelCloseInit::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics4ChannelMsg(ChannelMsg::ChannelCloseInit(
                    domain_msg,
                )))
            }
            chan_close_confirm::TYPE_URL => {
                let domain_msg =
                    chan_close_confirm::MsgChannelCloseConfirm::decode_vec(&any_msg.value)
                        .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics4ChannelMsg(
                    ChannelMsg::ChannelCloseConfirm(domain_msg),
                ))
            }
            // ICS04 packet messages
            recv_packet::TYPE_URL => {
                let domain_msg = recv_packet::MsgRecvPacket::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics4PacketMsg(PacketMsg::RecvPacket(
                    domain_msg,
                )))
            }
            acknowledgement::TYPE_URL => {
                let domain_msg = acknowledgement::MsgAcknowledgement::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics4PacketMsg(PacketMsg::AckPacket(
                    domain_msg,
                )))
            }
            timeout::TYPE_URL => {
                let domain_msg = timeout::MsgTimeout::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics4PacketMsg(PacketMsg::TimeoutPacket(
                    domain_msg,
                )))
            }
            timeout_on_close::TYPE_URL => {
                let domain_msg = timeout_on_close::MsgTimeoutOnClose::decode_vec(&any_msg.value)
                    .map_err(Error::malformed_message_bytes)?;
                Ok(Ics26Envelope::Ics4PacketMsg(
                    PacketMsg::TimeoutOnClosePacket(domain_msg),
                ))
            }
            _ => Err(Error::unknown_message_type_url(any_msg.type_url)),
        }
    }
}
