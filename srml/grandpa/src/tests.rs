// Copyright 2017-2019 Parity Technologies (UK) Ltd.
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

//! Tests for the module.

#![cfg(test)]

use primitives::testing::{Digest, Header as HeaderTest};
use primitives::traits::{Header, OnFinalize, ValidateUnsigned};
use substrate_primitives::{ed25519, H256, crypto::Pair};
use runtime_io::with_externalities;
use crate::mock::*;
use system::{EventRecord, Phase};
use codec::{Decode, Encode};
use rstd::hash::Hash;
use fg_primitives::{
	ScheduledChange, Equivocation, Precommit, Prevote,
	PrevoteEquivocation, PrecommitEquivocation
};
use super::*;

#[test]
fn authorities_change_logged() {
	with_externalities(&mut new_test_ext(vec![(1, 1), (2, 1), (3, 1)]), || {
		System::initialize(&1, &Default::default(), &Default::default(), &Default::default());
		Grandpa::schedule_change(to_authorities(vec![(4, 1), (5, 1), (6, 1)]), 0, None).unwrap();

		System::note_finished_extrinsics();
		Grandpa::on_finalize(1);

		let header = System::finalize();
		assert_eq!(header.digest, Digest {
			logs: vec![
				Signal::AuthoritiesChange(
					ScheduledChange { delay: 0, next_authorities: to_authorities(vec![(4, 1), (5, 1), (6, 1)]) }
				).into(),
			],
		});

		assert_eq!(System::events(), vec![
			EventRecord {
				phase: Phase::Finalization,
				event: Event::NewAuthorities(to_authorities(vec![(4, 1), (5, 1), (6, 1)])).into(),
				topics: vec![],
			},
		]);
	});
}

#[test]
fn authorities_change_logged_after_delay() {
	with_externalities(&mut new_test_ext(vec![(1, 1), (2, 1), (3, 1)]), || {
		System::initialize(&1, &Default::default(), &Default::default(), &Default::default());
		Grandpa::schedule_change(to_authorities(vec![(4, 1), (5, 1), (6, 1)]), 1, None).unwrap();
		Grandpa::on_finalize(1);
		let header = System::finalize();
		assert_eq!(header.digest, Digest {
			logs: vec![
				Signal::AuthoritiesChange(
					ScheduledChange { delay: 1, next_authorities: to_authorities(vec![(4, 1), (5, 1), (6, 1)]) }
				).into(),
			],
		});

		// no change at this height.
		assert_eq!(System::events(), vec![]);

		System::initialize(&2, &header.hash(), &Default::default(), &Default::default());
		System::note_finished_extrinsics();
		Grandpa::on_finalize(2);

		let _header = System::finalize();
		assert_eq!(System::events(), vec![
			EventRecord {
				phase: Phase::Finalization,
				event: Event::NewAuthorities(to_authorities(vec![(4, 1), (5, 1), (6, 1)])).into(),
				topics: vec![],
			},
		]);
	});
}

#[test]
fn cannot_schedule_change_when_one_pending() {
	with_externalities(&mut new_test_ext(vec![(1, 1), (2, 1), (3, 1)]), || {
		System::initialize(&1, &Default::default(), &Default::default(), &Default::default());
		Grandpa::schedule_change(to_authorities(vec![(4, 1), (5, 1), (6, 1)]), 1, None).unwrap();
		assert!(<PendingChange<Test>>::exists());
		assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_err());

		Grandpa::on_finalize(1);
		let header = System::finalize();

		System::initialize(&2, &header.hash(), &Default::default(), &Default::default());
		assert!(<PendingChange<Test>>::exists());
		assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_err());

		Grandpa::on_finalize(2);
		let header = System::finalize();

		System::initialize(&3, &header.hash(), &Default::default(), &Default::default());
		assert!(!<PendingChange<Test>>::exists());
		assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_ok());

		Grandpa::on_finalize(3);
		let _header = System::finalize();
	});
}

#[test]
fn new_decodes_from_old() {
	let old = OldStoredPendingChange {
		scheduled_at: 5u32,
		delay: 100u32,
		next_authorities: to_authorities(vec![(1, 5), (2, 10), (3, 2)]),
	};

	let encoded = old.encode();
	let new = StoredPendingChange::<u32>::decode(&mut &encoded[..]).unwrap();
	assert!(new.forced.is_none());
	assert_eq!(new.scheduled_at, old.scheduled_at);
	assert_eq!(new.delay, old.delay);
	assert_eq!(new.next_authorities, old.next_authorities);
}

#[test]
fn dispatch_forced_change() {
	with_externalities(&mut new_test_ext(vec![(1, 1), (2, 1), (3, 1)]), || {
		System::initialize(&1, &Default::default(), &Default::default(), &Default::default());
		Grandpa::schedule_change(
			to_authorities(vec![(4, 1), (5, 1), (6, 1)]),
			5,
			Some(0),
		).unwrap();

		assert!(<PendingChange<Test>>::exists());
		assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, Some(0)).is_err());

		Grandpa::on_finalize(1);
		let mut header = System::finalize();

		for i in 2..7 {
			System::initialize(&i, &header.hash(), &Default::default(), &Default::default());
			assert!(<PendingChange<Test>>::get().unwrap().forced.is_some());
			assert_eq!(Grandpa::next_forced(), Some(11));
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_err());
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, Some(0)).is_err());

			Grandpa::on_finalize(i);
			header = System::finalize();
		}

		// change has been applied at the end of block 6.
		// add a normal change.
		{
			System::initialize(&7, &header.hash(), &Default::default(), &Default::default());
			assert!(!<PendingChange<Test>>::exists());
			assert_eq!(Grandpa::grandpa_authorities(), to_authorities(vec![(4, 1), (5, 1), (6, 1)]));
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_ok());
			Grandpa::on_finalize(7);
			header = System::finalize();
		}

		// run the normal change.
		{
			System::initialize(&8, &header.hash(), &Default::default(), &Default::default());
			assert!(<PendingChange<Test>>::exists());
			assert_eq!(Grandpa::grandpa_authorities(), to_authorities(vec![(4, 1), (5, 1), (6, 1)]));
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_err());
			Grandpa::on_finalize(8);
			header = System::finalize();
		}

		// normal change applied. but we can't apply a new forced change for some
		// time.
		for i in 9..11 {
			System::initialize(&i, &header.hash(), &Default::default(), &Default::default());
			assert!(!<PendingChange<Test>>::exists());
			assert_eq!(Grandpa::grandpa_authorities(), to_authorities(vec![(5, 1)]));
			assert_eq!(Grandpa::next_forced(), Some(11));
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1), (6, 1)]), 5, Some(0)).is_err());
			Grandpa::on_finalize(i);
			header = System::finalize();
		}

		{
			System::initialize(&11, &header.hash(), &Default::default(), &Default::default());
			assert!(!<PendingChange<Test>>::exists());
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1), (6, 1), (7, 1)]), 5, Some(0)).is_ok());
			assert_eq!(Grandpa::next_forced(), Some(21));
			Grandpa::on_finalize(11);
			header = System::finalize();
		}
		let _ = header;
	});
}

#[test]
fn validate_unsigned_works() {
	let (pair, _seed) = ed25519::Pair::generate();
	let public = pair.public();
	let authorities = vec![(public.clone(), 1)];

	let hash1 = H256::random();
	let hash2 = H256::random();

	let prevote1 = Prevote { target_hash: hash1, target_number: 1 };
	let message1 = Message::Prevote(prevote1.clone());
	let payload1 = (message1, 0u64, 0u64).encode();
	let sig1 = pair.sign(payload1.as_slice());

	let prevote2 = Prevote { target_hash: hash2, target_number: 2 };
	let message2 = Message::Prevote(prevote2.clone());
	let payload2 = (message2, 0u64, 0u64).encode();
	let sig2 = pair.sign(payload2.as_slice());

	let proof1 = GrandpaEquivocationProof {
		set_id: 0,
		round: 0,
		equivocation: Equivocation {
			round_number: 0,
			identity: public.clone(),
			first: (prevote1.clone(), sig1.clone()),
			second: (prevote2.clone(), sig2.clone()),
		},
	};

	let proof2 = GrandpaEquivocationProof {
		set_id: 0,
		round: 0,
		equivocation: Equivocation {
			round_number: 0,
			identity: public.clone(),
			first: (prevote1.clone(), sig1.clone()),
			second: (prevote1.clone(), sig1.clone()),
		},
	};

	let proof3 = GrandpaEquivocationProof {
		set_id: 0,
		round: 0,
		equivocation: Equivocation {
			round_number: 0,
			identity: public.clone(),
			first: (prevote1.clone(), sig1.clone()),
			second: (prevote2.clone(), sig1.clone()),
		},
	};

	let call1 = Call::report_prevote_equivocation(proof1);
	let call2 = Call::report_prevote_equivocation(proof2);
	let call3 = Call::report_prevote_equivocation(proof3);

	with_externalities(&mut new_test_ext_sig(authorities), || {
		assert_eq!(Grandpa::validate_unsigned(&call1), TransactionValidity::Valid {
			priority: 10,
			requires: vec![],
			provides: vec![],
			longevity: 18446744073709551615,
			propagate: true,
		});
		assert_eq!(Grandpa::validate_unsigned(&call2), TransactionValidity::Invalid(0));
		assert_eq!(Grandpa::validate_unsigned(&call3), TransactionValidity::Invalid(0));
	});
}