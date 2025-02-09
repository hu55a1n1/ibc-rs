//! This module implements the processing logic for ICS4 (channel) messages.
use crate::events::{IbcEvent, ModuleEvent};
use crate::prelude::*;

use crate::core::ics04_channel::channel::ChannelEnd;
use crate::core::ics04_channel::context::ChannelReader;
use crate::core::ics04_channel::error::Error;
use crate::core::ics04_channel::msgs::ChannelMsg;
use crate::core::ics04_channel::packet::Packet;
use crate::core::ics04_channel::{msgs::PacketMsg, packet::PacketResult};
use crate::core::ics24_host::identifier::{ChannelId, ConnectionId, PortId};
use crate::core::ics26_routing::context::{
    Acknowledgement, Ics26Context, ModuleId, ModuleOutputBuilder, OnRecvPacketAck, Router,
};
use crate::handler::{HandlerOutput, HandlerOutputBuilder};

use super::channel::Counterparty;
use super::events::{CloseConfirm, CloseInit, OpenAck, OpenConfirm, OpenInit, OpenTry};
use super::Version;

pub mod acknowledgement;
pub mod chan_close_confirm;
pub mod chan_close_init;
pub mod chan_open_ack;
pub mod chan_open_confirm;
pub mod chan_open_init;
pub mod chan_open_try;
pub mod recv_packet;
pub mod send_packet;
pub mod timeout;
pub mod timeout_on_close;
pub mod verify;
pub mod write_acknowledgement;

/// Defines the possible states of a channel identifier in a `ChannelResult`.
#[derive(Clone, Debug)]
pub enum ChannelIdState {
    /// Specifies that the channel handshake handler allocated a new channel identifier. This
    /// happens during the processing of either the `MsgChannelOpenInit` or `MsgChannelOpenTry`.
    Generated,

    /// Specifies that the handler reused a previously-allocated channel identifier.
    Reused,
}

#[derive(Clone, Debug)]
pub struct ChannelResult {
    pub port_id: PortId,
    pub channel_id: ChannelId,
    pub channel_id_state: ChannelIdState,
    pub channel_end: ChannelEnd,
}

pub struct ModuleExtras {
    pub events: Vec<ModuleEvent>,
    pub log: Vec<String>,
}

impl ModuleExtras {
    pub fn empty() -> Self {
        ModuleExtras {
            events: Vec::new(),
            log: Vec::new(),
        }
    }
}

pub fn channel_validate<Ctx>(ctx: &Ctx, msg: &ChannelMsg) -> Result<ModuleId, Error>
where
    Ctx: Ics26Context,
{
    let module_id = msg.lookup_module(ctx)?;
    if ctx.router().has_route(&module_id) {
        Ok(module_id)
    } else {
        Err(Error::route_not_found())
    }
}

/// General entry point for processing any type of message related to the ICS4 channel open and
/// channel close handshake protocols.
pub fn channel_dispatch<Ctx>(
    ctx: &Ctx,
    msg: &ChannelMsg,
) -> Result<(Vec<String>, ChannelResult), Error>
where
    Ctx: ChannelReader,
{
    let output = match msg {
        ChannelMsg::ChannelOpenInit(msg) => chan_open_init::process(ctx, msg),
        ChannelMsg::ChannelOpenTry(msg) => chan_open_try::process(ctx, msg),
        ChannelMsg::ChannelOpenAck(msg) => chan_open_ack::process(ctx, msg),
        ChannelMsg::ChannelOpenConfirm(msg) => chan_open_confirm::process(ctx, msg),
        ChannelMsg::ChannelCloseInit(msg) => chan_close_init::process(ctx, msg),
        ChannelMsg::ChannelCloseConfirm(msg) => chan_close_confirm::process(ctx, msg),
    }?;

    let HandlerOutput { result, log, .. } = output;
    Ok((log, result))
}

pub fn channel_callback<Ctx>(
    ctx: &mut Ctx,
    module_id: &ModuleId,
    msg: &ChannelMsg,
    result: &mut ChannelResult,
) -> Result<ModuleExtras, Error>
where
    Ctx: Ics26Context,
{
    let cb = ctx
        .router_mut()
        .get_route_mut(module_id)
        .ok_or_else(Error::route_not_found)?;

    match msg {
        ChannelMsg::ChannelOpenInit(msg) => {
            let (extras, version) = cb.on_chan_open_init(
                msg.channel.ordering,
                &msg.channel.connection_hops,
                &msg.port_id,
                &result.channel_id,
                msg.channel.counterparty(),
                &msg.channel.version,
            )?;
            result.channel_end.version = version;

            Ok(extras)
        }
        ChannelMsg::ChannelOpenTry(msg) => {
            let (extras, version) = cb.on_chan_open_try(
                msg.channel.ordering,
                &msg.channel.connection_hops,
                &msg.port_id,
                &result.channel_id,
                msg.channel.counterparty(),
                &msg.counterparty_version,
            )?;
            result.channel_end.version = version;

            Ok(extras)
        }
        ChannelMsg::ChannelOpenAck(msg) => {
            cb.on_chan_open_ack(&msg.port_id, &result.channel_id, &msg.counterparty_version)
        }
        ChannelMsg::ChannelOpenConfirm(msg) => {
            cb.on_chan_open_confirm(&msg.port_id, &result.channel_id)
        }
        ChannelMsg::ChannelCloseInit(msg) => {
            cb.on_chan_close_init(&msg.port_id, &result.channel_id)
        }
        ChannelMsg::ChannelCloseConfirm(msg) => {
            cb.on_chan_close_confirm(&msg.port_id, &result.channel_id)
        }
    }
}

/// Constructs the proper channel event
pub fn channel_events(
    msg: &ChannelMsg,
    channel_id: ChannelId,
    counterparty: Counterparty,
    connection_id: ConnectionId,
    version: &Version,
) -> Vec<IbcEvent> {
    let event = match msg {
        ChannelMsg::ChannelOpenInit(msg) => IbcEvent::OpenInitChannel(OpenInit::new(
            msg.port_id.clone(),
            channel_id,
            counterparty.port_id,
            connection_id,
            version.clone(),
        )),
        ChannelMsg::ChannelOpenTry(msg) => IbcEvent::OpenTryChannel(OpenTry::new(
            msg.port_id.clone(),
            channel_id,
            counterparty.port_id,
            counterparty
                .channel_id
                .expect("counterparty channel id must exist after channel open try"),
            connection_id,
            version.clone(),
        )),
        ChannelMsg::ChannelOpenAck(msg) => IbcEvent::OpenAckChannel(OpenAck::new(
            msg.port_id.clone(),
            channel_id,
            counterparty.port_id,
            counterparty
                .channel_id
                .expect("counterparty channel id must exist after channel open ack"),
            connection_id,
        )),
        ChannelMsg::ChannelOpenConfirm(msg) => IbcEvent::OpenConfirmChannel(OpenConfirm::new(
            msg.port_id.clone(),
            channel_id,
            counterparty.port_id,
            counterparty
                .channel_id
                .expect("counterparty channel id must exist after channel open confirm"),
            connection_id,
        )),
        ChannelMsg::ChannelCloseInit(msg) => IbcEvent::CloseInitChannel(CloseInit::new(
            msg.port_id.clone(),
            channel_id,
            counterparty.port_id,
            counterparty
                .channel_id
                .expect("counterparty channel id must exist after channel open ack"),
            connection_id,
        )),
        ChannelMsg::ChannelCloseConfirm(msg) => IbcEvent::CloseConfirmChannel(CloseConfirm::new(
            msg.port_id.clone(),
            channel_id,
            counterparty.port_id,
            counterparty
                .channel_id
                .expect("counterparty channel id must exist after channel open ack"),
            connection_id,
        )),
    };

    vec![event]
}

pub fn get_module_for_packet_msg<Ctx>(ctx: &Ctx, msg: &PacketMsg) -> Result<ModuleId, Error>
where
    Ctx: Ics26Context,
{
    let module_id = match msg {
        PacketMsg::RecvPacket(msg) => ctx
            .lookup_module_by_port(&msg.packet.destination_port)
            .map_err(Error::ics05_port)?,
        PacketMsg::AckPacket(msg) => ctx
            .lookup_module_by_port(&msg.packet.source_port)
            .map_err(Error::ics05_port)?,
        PacketMsg::TimeoutPacket(msg) => ctx
            .lookup_module_by_port(&msg.packet.source_port)
            .map_err(Error::ics05_port)?,
        PacketMsg::TimeoutOnClosePacket(msg) => ctx
            .lookup_module_by_port(&msg.packet.source_port)
            .map_err(Error::ics05_port)?,
    };

    if ctx.router().has_route(&module_id) {
        Ok(module_id)
    } else {
        Err(Error::route_not_found())
    }
}

/// Dispatcher for processing any type of message related to the ICS4 packet protocols.
pub fn packet_dispatch<Ctx>(
    ctx: &Ctx,
    msg: &PacketMsg,
) -> Result<(HandlerOutputBuilder<()>, PacketResult), Error>
where
    Ctx: ChannelReader,
{
    let output = match msg {
        PacketMsg::RecvPacket(msg) => recv_packet::process(ctx, msg),
        PacketMsg::AckPacket(msg) => acknowledgement::process(ctx, msg),
        PacketMsg::TimeoutPacket(msg) => timeout::process(ctx, msg),
        PacketMsg::TimeoutOnClosePacket(msg) => timeout_on_close::process(ctx, msg),
    }?;
    let HandlerOutput {
        result,
        log,
        events,
    } = output;
    let builder = HandlerOutput::builder().with_log(log).with_events(events);
    Ok((builder, result))
}

pub fn packet_callback<Ctx>(
    ctx: &mut Ctx,
    module_id: &ModuleId,
    msg: &PacketMsg,
    output: &mut HandlerOutputBuilder<()>,
) -> Result<(), Error>
where
    Ctx: Ics26Context,
{
    let mut module_output = ModuleOutputBuilder::new();
    let mut core_output = HandlerOutputBuilder::new();

    let result = do_packet_callback(ctx, module_id, msg, &mut module_output, &mut core_output);
    output.merge(module_output);
    output.merge(core_output);

    result
}

fn do_packet_callback(
    ctx: &mut impl Ics26Context,
    module_id: &ModuleId,
    msg: &PacketMsg,
    module_output: &mut ModuleOutputBuilder,
    core_output: &mut HandlerOutputBuilder<()>,
) -> Result<(), Error> {
    let cb = ctx
        .router_mut()
        .get_route_mut(module_id)
        .ok_or_else(Error::route_not_found)?;

    match msg {
        PacketMsg::RecvPacket(msg) => {
            let result = cb.on_recv_packet(module_output, &msg.packet, &msg.signer);
            match result {
                OnRecvPacketAck::Nil(write_fn) => {
                    write_fn(cb.as_any_mut()).map_err(Error::app_module)
                }
                OnRecvPacketAck::Successful(ack, write_fn) => {
                    write_fn(cb.as_any_mut()).map_err(Error::app_module)?;

                    process_write_ack(ctx, msg.packet.clone(), ack.as_ref(), core_output)
                }
                OnRecvPacketAck::Failed(ack) => {
                    process_write_ack(ctx, msg.packet.clone(), ack.as_ref(), core_output)
                }
            }
        }
        PacketMsg::AckPacket(msg) => cb.on_acknowledgement_packet(
            module_output,
            &msg.packet,
            &msg.acknowledgement,
            &msg.signer,
        ),
        PacketMsg::TimeoutPacket(msg) => {
            cb.on_timeout_packet(module_output, &msg.packet, &msg.signer)
        }
        PacketMsg::TimeoutOnClosePacket(msg) => {
            cb.on_timeout_packet(module_output, &msg.packet, &msg.signer)
        }
    }
}

fn process_write_ack(
    ctx: &mut impl Ics26Context,
    packet: Packet,
    acknowledgement: &dyn Acknowledgement,
    core_output: &mut HandlerOutputBuilder<()>,
) -> Result<(), Error> {
    let HandlerOutput {
        result,
        log,
        events,
    } = write_acknowledgement::process(ctx, packet, acknowledgement.as_ref().to_vec().into())?;

    // store write ack result
    ctx.store_packet_result(result)?;

    core_output.merge_output(
        HandlerOutput::builder()
            .with_log(log)
            .with_events(events)
            .with_result(()),
    );

    Ok(())
}
