use sha2::{Digest, Sha256};

use super::error::Error as Ics20Error;
use crate::applications::transfer::acknowledgement::Acknowledgement;
use crate::applications::transfer::events::{AckEvent, AckStatusEvent, RecvEvent, TimeoutEvent};
use crate::applications::transfer::packet::PacketData;
use crate::applications::transfer::relay::on_ack_packet::process_ack_packet;
use crate::applications::transfer::relay::on_recv_packet::process_recv_packet;
use crate::applications::transfer::relay::on_timeout_packet::process_timeout_packet;
use crate::applications::transfer::{PrefixedCoin, PrefixedDenom, VERSION};
use crate::core::ics04_channel::channel::{Counterparty, Order};
use crate::core::ics04_channel::context::{ChannelKeeper, ChannelReader};
use crate::core::ics04_channel::handler::ModuleExtras;
use crate::core::ics04_channel::msgs::acknowledgement::Acknowledgement as GenericAcknowledgement;
use crate::core::ics04_channel::packet::Packet;
use crate::core::ics04_channel::Version;
use crate::core::ics24_host::identifier::{ChannelId, ConnectionId, PortId};
use crate::core::ics26_routing::context::{ModuleOutputBuilder, OnRecvPacketAck};
use crate::prelude::*;
use crate::signer::Signer;

pub trait Ics20Keeper:
    ChannelKeeper + BankKeeper<AccountId = <Self as Ics20Keeper>::AccountId>
{
    type AccountId;
}

pub trait Ics20Reader: ChannelReader {
    type AccountId: TryFrom<Signer>;

    /// get_port returns the portID for the transfer module.
    fn get_port(&self) -> Result<PortId, Ics20Error>;

    /// Returns the escrow account id for a port and channel combination
    fn get_channel_escrow_address(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<<Self as Ics20Reader>::AccountId, Ics20Error>;

    /// Returns true iff send is enabled.
    fn is_send_enabled(&self) -> bool;

    /// Returns true iff receive is enabled.
    fn is_receive_enabled(&self) -> bool;

    /// Returns a hash of the prefixed denom.
    /// Implement only if the host chain supports hashed denominations.
    fn denom_hash_string(&self, _denom: &PrefixedDenom) -> Option<String> {
        None
    }
}

// https://github.com/cosmos/cosmos-sdk/blob/master/docs/architecture/adr-028-public-key-addresses.md
pub fn cosmos_adr028_escrow_address(port_id: &PortId, channel_id: &ChannelId) -> Vec<u8> {
    let contents = format!("{}/{}", port_id, channel_id);

    let mut hasher = Sha256::new();
    hasher.update(VERSION.as_bytes());
    hasher.update([0]);
    hasher.update(contents.as_bytes());

    let mut hash = hasher.finalize().to_vec();
    hash.truncate(20);
    hash
}

pub trait BankKeeper {
    type AccountId;

    /// This function should enable sending ibc fungible tokens from one account to another
    fn send_coins(
        &mut self,
        from: &Self::AccountId,
        to: &Self::AccountId,
        amt: &PrefixedCoin,
    ) -> Result<(), Ics20Error>;

    /// This function to enable minting ibc tokens to a user account
    fn mint_coins(
        &mut self,
        account: &Self::AccountId,
        amt: &PrefixedCoin,
    ) -> Result<(), Ics20Error>;

    /// This function should enable burning of minted tokens in a user account
    fn burn_coins(
        &mut self,
        account: &Self::AccountId,
        amt: &PrefixedCoin,
    ) -> Result<(), Ics20Error>;
}

/// Captures all the dependencies which the ICS20 module requires to be able to dispatch and
/// process IBC messages.
pub trait Ics20Context:
    Ics20Keeper<AccountId = <Self as Ics20Context>::AccountId>
    + Ics20Reader<AccountId = <Self as Ics20Context>::AccountId>
{
    type AccountId: TryFrom<Signer>;
}

#[allow(clippy::too_many_arguments)]
pub fn on_chan_open_init(
    ctx: &mut impl Ics20Context,
    order: Order,
    _connection_hops: &[ConnectionId],
    port_id: &PortId,
    _channel_id: &ChannelId,
    _counterparty: &Counterparty,
    version: &Version,
) -> Result<(ModuleExtras, Version), Ics20Error> {
    if order != Order::Unordered {
        return Err(Ics20Error::channel_not_unordered(order));
    }
    let bound_port = ctx.get_port()?;
    if port_id != &bound_port {
        return Err(Ics20Error::invalid_port(port_id.clone(), bound_port));
    }

    if !version.is_empty() && version != &Version::ics20() {
        return Err(Ics20Error::invalid_version(version.clone()));
    }

    Ok((ModuleExtras::empty(), Version::ics20()))
}

#[allow(clippy::too_many_arguments)]
pub fn on_chan_open_try(
    _ctx: &mut impl Ics20Context,
    order: Order,
    _connection_hops: &[ConnectionId],
    _port_id: &PortId,
    _channel_id: &ChannelId,
    _counterparty: &Counterparty,
    counterparty_version: &Version,
) -> Result<(ModuleExtras, Version), Ics20Error> {
    if order != Order::Unordered {
        return Err(Ics20Error::channel_not_unordered(order));
    }
    if counterparty_version != &Version::ics20() {
        return Err(Ics20Error::invalid_counterparty_version(
            counterparty_version.clone(),
        ));
    }

    Ok((ModuleExtras::empty(), Version::ics20()))
}

pub fn on_chan_open_ack(
    _ctx: &mut impl Ics20Context,
    _port_id: &PortId,
    _channel_id: &ChannelId,
    counterparty_version: &Version,
) -> Result<ModuleExtras, Ics20Error> {
    if counterparty_version != &Version::ics20() {
        return Err(Ics20Error::invalid_counterparty_version(
            counterparty_version.clone(),
        ));
    }

    Ok(ModuleExtras::empty())
}

pub fn on_chan_open_confirm(
    _ctx: &mut impl Ics20Context,
    _port_id: &PortId,
    _channel_id: &ChannelId,
) -> Result<ModuleExtras, Ics20Error> {
    Ok(ModuleExtras::empty())
}

pub fn on_chan_close_init(
    _ctx: &mut impl Ics20Context,
    _port_id: &PortId,
    _channel_id: &ChannelId,
) -> Result<ModuleExtras, Ics20Error> {
    Err(Ics20Error::cant_close_channel())
}

pub fn on_chan_close_confirm(
    _ctx: &mut impl Ics20Context,
    _port_id: &PortId,
    _channel_id: &ChannelId,
) -> Result<ModuleExtras, Ics20Error> {
    Ok(ModuleExtras::empty())
}

pub fn on_recv_packet<Ctx: 'static + Ics20Context>(
    ctx: &Ctx,
    output: &mut ModuleOutputBuilder,
    packet: &Packet,
    _relayer: &Signer,
) -> OnRecvPacketAck {
    let data = match serde_json::from_slice::<PacketData>(&packet.data) {
        Ok(data) => data,
        Err(_) => {
            return OnRecvPacketAck::Failed(Box::new(Acknowledgement::Error(
                Ics20Error::packet_data_deserialization().to_string(),
            )))
        }
    };

    let ack = match process_recv_packet(ctx, output, packet, data.clone()) {
        Ok(write_fn) => OnRecvPacketAck::Successful(Box::new(Acknowledgement::success()), write_fn),
        Err(e) => OnRecvPacketAck::Failed(Box::new(Acknowledgement::from_error(e))),
    };

    let recv_event = RecvEvent {
        receiver: data.receiver,
        denom: data.token.denom,
        amount: data.token.amount,
        success: ack.is_successful(),
    };
    output.emit(recv_event.into());

    ack
}

pub fn on_acknowledgement_packet(
    ctx: &mut impl Ics20Context,
    output: &mut ModuleOutputBuilder,
    packet: &Packet,
    acknowledgement: &GenericAcknowledgement,
    _relayer: &Signer,
) -> Result<(), Ics20Error> {
    let data = serde_json::from_slice::<PacketData>(&packet.data)
        .map_err(|_| Ics20Error::packet_data_deserialization())?;

    let acknowledgement = serde_json::from_slice::<Acknowledgement>(acknowledgement.as_ref())
        .map_err(|_| Ics20Error::ack_deserialization())?;

    process_ack_packet(ctx, packet, &data, &acknowledgement)?;

    let ack_event = AckEvent {
        receiver: data.receiver,
        denom: data.token.denom,
        amount: data.token.amount,
        acknowledgement: acknowledgement.clone(),
    };
    output.emit(ack_event.into());
    output.emit(AckStatusEvent { acknowledgement }.into());

    Ok(())
}

pub fn on_timeout_packet(
    ctx: &mut impl Ics20Context,
    output: &mut ModuleOutputBuilder,
    packet: &Packet,
    _relayer: &Signer,
) -> Result<(), Ics20Error> {
    let data = serde_json::from_slice::<PacketData>(&packet.data)
        .map_err(|_| Ics20Error::packet_data_deserialization())?;

    process_timeout_packet(ctx, packet, &data)?;

    let timeout_event = TimeoutEvent {
        refund_receiver: data.sender,
        refund_denom: data.token.denom,
        refund_amount: data.token.amount,
    };
    output.emit(timeout_event.into());

    Ok(())
}

#[cfg(test)]
pub(crate) mod test {
    use subtle_encoding::bech32;

    use crate::applications::transfer::context::{cosmos_adr028_escrow_address, on_chan_open_try};
    use crate::applications::transfer::error::Error as Ics20Error;
    use crate::applications::transfer::msgs::transfer::MsgTransfer;
    use crate::applications::transfer::relay::send_transfer::send_transfer;
    use crate::applications::transfer::PrefixedCoin;
    use crate::core::ics04_channel::channel::{Counterparty, Order};
    use crate::core::ics04_channel::error::Error;
    use crate::core::ics04_channel::Version;
    use crate::core::ics24_host::identifier::{ChannelId, ConnectionId, PortId};
    use crate::handler::HandlerOutputBuilder;
    use crate::prelude::*;
    use crate::test_utils::{get_dummy_transfer_module, DummyTransferModule};

    use super::on_chan_open_init;

    pub(crate) fn deliver(
        ctx: &mut DummyTransferModule,
        output: &mut HandlerOutputBuilder<()>,
        msg: MsgTransfer<PrefixedCoin>,
    ) -> Result<(), Error> {
        send_transfer(ctx, output, msg).map_err(|e: Ics20Error| Error::app_module(e.to_string()))
    }

    fn get_defaults() -> (
        DummyTransferModule,
        Order,
        Vec<ConnectionId>,
        PortId,
        ChannelId,
        Counterparty,
    ) {
        let ctx = get_dummy_transfer_module();
        let order = Order::Unordered;
        let connection_hops = vec![ConnectionId::new(1)];
        let port_id = PortId::transfer();
        let channel_id = ChannelId::new(1);
        let counterparty = Counterparty::new(port_id.clone(), Some(channel_id.clone()));

        (
            ctx,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
        )
    }

    #[test]
    fn test_cosmos_escrow_address() {
        fn assert_eq_escrow_address(port_id: &str, channel_id: &str, address: &str) {
            let port_id = port_id.parse().unwrap();
            let channel_id = channel_id.parse().unwrap();
            let gen_address = {
                let addr = cosmos_adr028_escrow_address(&port_id, &channel_id);
                bech32::encode("cosmos", addr)
            };
            assert_eq!(gen_address, address.to_owned())
        }

        // addresses obtained using `gaiad query ibc-transfer escrow-address [port-id] [channel-id]`
        assert_eq_escrow_address(
            "transfer",
            "channel-141",
            "cosmos1x54ltnyg88k0ejmk8ytwrhd3ltm84xehrnlslf",
        );
        assert_eq_escrow_address(
            "transfer",
            "channel-207",
            "cosmos1ju6tlfclulxumtt2kglvnxduj5d93a64r5czge",
        );
        assert_eq_escrow_address(
            "transfer",
            "channel-187",
            "cosmos177x69sver58mcfs74x6dg0tv6ls4s3xmmcaw53",
        );
    }

    /// If the relayer passed "", indicating that it wants us to return the versions we support.
    /// We currently only support ics20
    #[test]
    fn test_on_chan_open_init_empty_version() {
        let (mut ctx, order, connection_hops, port_id, channel_id, counterparty) = get_defaults();

        let in_version = Version::new("".to_string());

        let (_, out_version) = on_chan_open_init(
            &mut ctx,
            order,
            &connection_hops,
            &port_id,
            &channel_id,
            &counterparty,
            &in_version,
        )
        .unwrap();

        assert_eq!(out_version, Version::ics20());
    }

    /// If the relayer passed in the only supported version (ics20), then return ics20
    #[test]
    fn test_on_chan_open_init_ics20_version() {
        let (mut ctx, order, connection_hops, port_id, channel_id, counterparty) = get_defaults();

        let in_version = Version::ics20();
        let (_, out_version) = on_chan_open_init(
            &mut ctx,
            order,
            &connection_hops,
            &port_id,
            &channel_id,
            &counterparty,
            &in_version,
        )
        .unwrap();

        assert_eq!(out_version, Version::ics20());
    }

    /// If the relayer passed in an unsupported version, then fail
    #[test]
    fn test_on_chan_open_init_incorrect_version() {
        let (mut ctx, order, connection_hops, port_id, channel_id, counterparty) = get_defaults();

        let in_version = Version::new("some-unsupported-version".to_string());
        let res = on_chan_open_init(
            &mut ctx,
            order,
            &connection_hops,
            &port_id,
            &channel_id,
            &counterparty,
            &in_version,
        );

        assert!(res.is_err());
    }

    /// If the counterparty supports ics20, then return ics20
    #[test]
    fn test_on_chan_open_try_counterparty_correct_version() {
        let (mut ctx, order, connection_hops, port_id, channel_id, counterparty) = get_defaults();

        let counterparty_version = Version::ics20();

        let (_, out_version) = on_chan_open_try(
            &mut ctx,
            order,
            &connection_hops,
            &port_id,
            &channel_id,
            &counterparty,
            &counterparty_version,
        )
        .unwrap();

        assert_eq!(out_version, Version::ics20());
    }

    /// If the counterparty doesn't support ics20, then fail
    #[test]
    fn test_on_chan_open_try_counterparty_incorrect_version() {
        let (mut ctx, order, connection_hops, port_id, channel_id, counterparty) = get_defaults();

        let counterparty_version = Version::new("some-unsupported-version".to_string());

        let res = on_chan_open_try(
            &mut ctx,
            order,
            &connection_hops,
            &port_id,
            &channel_id,
            &counterparty,
            &counterparty_version,
        );

        assert!(res.is_err());
    }
}
