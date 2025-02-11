// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use srml_support::runtime_primitives::generic;
use srml_support::runtime_primitives::traits::{BlakeTwo256, Block as _, Verify};
use srml_support::codec::{Encode, Decode};
use primitives::{H256, sr25519};
use serde::{Serialize, Deserialize};

mod system;

mod module {
	use super::*;

	pub type Request<T> = (
		<T as system::Trait>::AccountId,
		Role,
		<T as system::Trait>::BlockNumber,
	);
	pub type Requests<T> = Vec<Request<T>>;

	#[derive(Encode, Decode, Copy, Clone, Eq, PartialEq, Debug)]
	pub enum Role {
		Storage,
	}

	#[derive(Encode, Decode, Copy, Clone, Eq, PartialEq, Debug)]
	pub struct RoleParameters<T: Trait> {
		// minimum actors to maintain - if role is unstaking
		// and remaining actors would be less that this value - prevent or punish for unstaking
		pub min_actors: u32,

		// the maximum number of spots available to fill for a role
		pub max_actors: u32,

		// payouts are made at this block interval
		pub reward_period: T::BlockNumber,

		// minimum amount of time before being able to unstake
		pub bonding_period: T::BlockNumber,

		// how long tokens remain locked for after unstaking
		pub unbonding_period: T::BlockNumber,

		// minimum period required to be in service. unbonding before this time is highly penalized
		pub min_service_period: T::BlockNumber,

		// "startup" time allowed for roles that need to sync their infrastructure
		// with other providers before they are considered in service and punishable for
		// not delivering required level of service.
		pub startup_grace_period: T::BlockNumber,
	}

	impl<T: Trait> Default for RoleParameters<T> {
		fn default() -> Self {
			Self {
				max_actors: 10,
				reward_period: T::BlockNumber::default(),
				unbonding_period: T::BlockNumber::default(),

				// not currently used
				min_actors: 5,
				bonding_period: T::BlockNumber::default(),
				min_service_period: T::BlockNumber::default(),
				startup_grace_period: T::BlockNumber::default(),
			}
		}
	}

	pub trait Trait: system::Trait {}

	srml_support::decl_module! {
		pub struct Module<T: Trait> for enum Call where origin: T::Origin {}
	}

	#[derive(Encode, Decode, Copy, Clone, Serialize, Deserialize)]
	pub struct Data<T: Trait> {
		pub	data: T::BlockNumber,
	}

	impl<T: Trait> Default for Data<T> {
		fn default() -> Self {
			Self {
				data: T::BlockNumber::default(),
			}
		}
	}

	srml_support::decl_storage! {
		trait Store for Module<T: Trait> as Actors {
			/// requirements to enter and maintain status in roles
			pub Parameters get(parameters) build(|config: &GenesisConfig| {
				if config.enable_storage_role {
					let storage_params: RoleParameters<T> = Default::default();
					vec![(Role::Storage, storage_params)]
				} else {
					vec![]
				}
			}): map Role => Option<RoleParameters<T>>;

			/// the roles members can enter into
			pub AvailableRoles get(available_roles) build(|config: &GenesisConfig| {
				if config.enable_storage_role {
					vec![(Role::Storage)]
				} else {
					vec![]
				}
			}): Vec<Role>;

			/// Actors list
			pub ActorAccountIds get(actor_account_ids) : Vec<T::AccountId>;

			/// actor accounts associated with a role
			pub AccountIdsByRole get(account_ids_by_role) : map Role => Vec<T::AccountId>;

			/// tokens locked until given block number
			pub Bondage get(bondage) : map T::AccountId => T::BlockNumber;

			/// First step before enter a role is registering intent with a new account/key.
			/// This is done by sending a role_entry_request() from the new account.
			/// The member must then send a stake() transaction to approve the request and enter the desired role.
			/// The account making the request will be bonded and must have
			/// sufficient balance to cover the minimum stake for the role.
			/// Bonding only occurs after successful entry into a role.
			pub RoleEntryRequests get(role_entry_requests) : Requests<T>;

			/// Entry request expires after this number of blocks
			pub RequestLifeTime get(request_life_time) config(request_life_time) : u64 = 0;
		}
		add_extra_genesis {
			config(enable_storage_role): bool;
		}
	}
}

pub type Signature = sr25519::Signature;
pub type AccountId = <Signature as Verify>::Signer;
pub type BlockNumber = u64;
pub type Index = u64;
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<u32, Call, Signature, ()>;

impl system::Trait for Runtime {
	type Hash = H256;
	type Origin = Origin;
	type BlockNumber = BlockNumber;
	type AccountId = AccountId;
	type Event = Event;
}

impl module::Trait for Runtime {}

srml_support::construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		System: system::{Module, Call, Event},
		Module: module::{Module, Call, Storage, Config},
	}
);

#[test]
fn create_genesis_config() {
	GenesisConfig {
		module: Some(module::GenesisConfig {
			request_life_time: 0,
			enable_storage_role: true,
		})
	};
}
