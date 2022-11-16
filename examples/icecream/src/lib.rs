//! # A smart contract for buying icecream safely
//!
//! This contract solves a very realistic problem related to icecream.
//! Imagine you want to purchase icecream from a vendor, and that you hate
//! eating icecream when it's raining. This contract solves the problem by
//! acting as a middleman which only allows your transfer to the icecream vendor
//! to go through if the sun is shining.
//!
//! The icecream contract relies on a weather service contract to determine the
//! weather. Both contracts are included in this module.
//!
//!
//! ## The Icecream Contract
//!
//! The contract is initialised with a contract address to the weather service
//! contract.
//!
//! Its primary function is `buy_icecream`, which works as follows:
//!  - It is called with an `AccountAddress` of the icecream vendor and the
//!    icecream price as amount.
//!  - It queries the `Weather` from the weather_service contract.
//!  - If it's `Weather::Sunny`, the transfer goes through to the icecream
//!    vendor.
//!  - Otherwise, the amount is returned to invoker.
//!
//! It also has a `replace_weather_service` function, in which the owner can
//! replace the weather service.
//!
//!
//! ## The Weather Service Contract
//!
//! The contract is initialised with the `Weather`.
//!
//! It has `get` and `set` receive functions, which either return or set the
//! weather. Only the owner can update the weather.

#![cfg_attr(not(feature = "std"), no_std)]
use concordium_std::*;

#[derive(Serialize, SchemaType, Clone)]
struct State {
    weather_service: ContractAddress,
}

#[derive(Serialize, SchemaType, Clone, Copy)]
enum Weather {
    Rainy,
    Sunny,
}

/// The custom errors the contract can produce.
#[derive(Serialize, Debug, PartialEq, Eq, Reject, SchemaType)]
enum ContractError {
    /// Failed parsing the parameter.
    #[from(ParseError)]
    ParseParams,
    /// Failed account transfer.
    #[from(TransferError)]
    TransferError,
    /// Failed contract invoke.
    ContractError,
    Unauthenticated,
}

impl<A> From<CallContractError<A>> for ContractError {
    fn from(_: CallContractError<A>) -> Self { Self::ContractError }
}

type ContractResult<A> = Result<A, ContractError>;

/// Initialise the contract with the contract address of the weather service.
#[init(contract = "icecream", parameter = "ContractAddress")]
fn contract_init<S: HasStateApi>(
    ctx: &impl HasInitContext,
    _state_builder: &mut StateBuilder<S>,
) -> InitResult<State> {
    let weather_service: ContractAddress = ctx.parameter_cursor().get()?;
    Ok(State {
        weather_service,
    })
}

/// Attempt purchasing icecream from the icecream vendor.
#[receive(
    contract = "icecream",
    name = "buy_icecream",
    parameter = "AccountAddress",
    payable,
    mutable,
    error = "ContractError"
)]
fn contract_buy_icecream<S: HasStateApi>(
    ctx: &impl HasReceiveContext,
    host: &mut impl HasHost<State, StateApiType = S>,
    amount: Amount,
) -> ContractResult<()> {
    let weather_service = host.state().weather_service;
    let icecream_vendor: AccountAddress = ctx.parameter_cursor().get()?;

    let weather = host
        .invoke_contract_raw(
            &weather_service,
            Parameter(&[]),
            EntrypointName::new_unchecked("get"),
            Amount::zero(),
        )?
        .1;
    let weather = if let Some(mut weather) = weather {
        weather.get()?
    } else {
        return Err(ContractError::ContractError);
    };

    match weather {
        Weather::Rainy => {
            host.invoke_transfer(&ctx.invoker(), amount)?;
            // We could also abort here, but this is useful to show off some
            // testing features.
        }
        Weather::Sunny => host.invoke_transfer(&icecream_vendor, amount)?,
    }
    Ok(())
}

/// Replace the weather service with another.
/// Only the owner of the contract can do so.
#[receive(
    contract = "icecream",
    name = "replace_weather_service",
    parameter = "ContractAddress",
    mutable,
    error = "ContractError"
)]
fn contract_replace_weather_service<S: HasStateApi>(
    ctx: &impl HasReceiveContext,
    host: &mut impl HasHost<State, StateApiType = S>,
) -> ContractResult<()> {
    ensure_eq!(Address::Account(ctx.owner()), ctx.sender(), ContractError::Unauthenticated);
    let new_weather_service: ContractAddress = ctx.parameter_cursor().get()?;
    host.state_mut().weather_service = new_weather_service;
    Ok(())
}

//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Initialse the weather service with the weather.
#[init(contract = "weather", parameter = "Weather")]
fn weather_init<S: HasStateApi>(
    ctx: &impl HasInitContext,
    _state_builder: &mut StateBuilder<S>,
) -> InitResult<Weather> {
    let weather = ctx.parameter_cursor().get()?;
    Ok(weather)
}

/// Get the current weather.
#[receive(contract = "weather", name = "get", return_value = "Weather", error = "ContractError")]
fn weather_get<S: HasStateApi>(
    _ctx: &impl HasReceiveContext,
    host: &impl HasHost<Weather, StateApiType = S>,
) -> ContractResult<Weather> {
    Ok(*host.state())
}

/// Update the weather.
#[receive(
    contract = "weather",
    name = "set",
    parameter = "Weather",
    mutable,
    error = "ContractError"
)]
fn weather_set<S: HasStateApi>(
    ctx: &impl HasReceiveContext,
    host: &mut impl HasHost<Weather, StateApiType = S>,
) -> ContractResult<()> {
    ensure_eq!(Address::Account(ctx.owner()), ctx.sender(), ContractError::Unauthenticated); // Only the owner can update the weather.
    *host.state_mut() = ctx.parameter_cursor().get()?;
    Ok(())
}

//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

#[concordium_cfg_test]
mod tests {
    use super::*;
    use test_infrastructure::*;

    const INVOKER_ADDR: AccountAddress = AccountAddress([0; 32]);
    const WEATHER_SERVICE: ContractAddress = ContractAddress {
        index:    1,
        subindex: 0,
    };
    const ICECREAM_VENDOR: AccountAddress = AccountAddress([1; 32]);
    const ICECREAM_PRICE: Amount = Amount {
        micro_ccd: 6000000, // 6 CCD
    };

    #[concordium_test]
    fn test_sunny_days() {
        // Arrange
        let mut ctx = TestReceiveContext::empty();
        let state = State {
            weather_service: WEATHER_SERVICE,
        };
        let mut host = TestHost::new(state, TestStateBuilder::new());

        // Set up context
        let parameter = to_bytes(&ICECREAM_VENDOR);
        ctx.set_owner(INVOKER_ADDR);
        ctx.set_invoker(INVOKER_ADDR);
        ctx.set_parameter(&parameter);
        host.set_self_balance(ICECREAM_PRICE); // This should be the balance prior to the call plus the incoming amount.

        // Set up a mock invocation for the weather service.
        host.setup_mock_entrypoint(
            WEATHER_SERVICE,
            OwnedEntrypointName::new_unchecked("get".into()),
            MockFn::returning_ok(Weather::Sunny),
        );

        // Act
        contract_buy_icecream(&ctx, &mut host, ICECREAM_PRICE)
            .expect_report("Calling buy_icecream failed.");

        // Assert
        assert!(host.transfer_occurred(&ICECREAM_VENDOR, ICECREAM_PRICE));
        assert!(host.get_transfers_to(INVOKER_ADDR).is_empty()); // Check that
                                                                 // no
                                                                 // transfers to
                                                                 // the invoker
                                                                 // occured.
    }

    #[concordium_test]
    fn test_rainy_days() {
        // Arrange
        let mut ctx = TestReceiveContext::empty();
        let state = State {
            weather_service: WEATHER_SERVICE,
        };
        let mut host = TestHost::new(state, TestStateBuilder::new());

        // Set up context
        let parameter = to_bytes(&ICECREAM_VENDOR);
        ctx.set_owner(INVOKER_ADDR);
        ctx.set_invoker(INVOKER_ADDR);
        ctx.set_parameter(&parameter);
        host.set_self_balance(ICECREAM_PRICE);

        // Set up mock invocation
        host.setup_mock_entrypoint(
            WEATHER_SERVICE,
            OwnedEntrypointName::new_unchecked("get".into()),
            MockFn::returning_ok(Weather::Rainy),
        );

        // Act
        contract_buy_icecream(&ctx, &mut host, ICECREAM_PRICE)
            .expect_report("Calling buy_icecream failed.");

        // Assert
        assert!(host.transfer_occurred(&INVOKER_ADDR, ICECREAM_PRICE));
        assert_eq!(host.get_transfers(), &[(INVOKER_ADDR, ICECREAM_PRICE)]); // Check that this is the only transfer.
    }

    #[concordium_test]
    fn test_missing_icecream_vendor() {
        // Arrange
        let mut ctx = TestReceiveContext::empty();
        let state = State {
            weather_service: WEATHER_SERVICE,
        };
        let mut host = TestHost::new(state, TestStateBuilder::new());

        // Set up context
        let parameter = to_bytes(&ICECREAM_VENDOR);
        ctx.set_owner(INVOKER_ADDR);
        ctx.set_invoker(INVOKER_ADDR);
        ctx.set_parameter(&parameter);
        host.set_self_balance(ICECREAM_PRICE);

        // By default all transfers to accounts will work, but here we want to test what
        // happens when the vendor account doesn't exist.
        host.make_account_missing(ICECREAM_VENDOR);

        // Set up mock invocation
        host.setup_mock_entrypoint(
            WEATHER_SERVICE,
            OwnedEntrypointName::new_unchecked("get".into()),
            MockFn::returning_ok(Weather::Sunny),
        );

        // Act + Assert
        let result = contract_buy_icecream(&ctx, &mut host, ICECREAM_PRICE);
        claim_eq!(result, Err(ContractError::TransferError));
    }

    #[concordium_test]
    fn test_missing_weather_service() {
        // Arrange
        let mut ctx = TestReceiveContext::empty();
        let state = State {
            weather_service: WEATHER_SERVICE,
        };
        let mut host = TestHost::new(state, TestStateBuilder::new());

        // Set up context
        let parameter = to_bytes(&ICECREAM_VENDOR);
        ctx.set_owner(INVOKER_ADDR);
        ctx.set_parameter(&parameter);

        // Set up mock invocation
        host.setup_mock_entrypoint(
            WEATHER_SERVICE,
            OwnedEntrypointName::new_unchecked("get".into()),
            MockFn::returning_err::<()>(CallContractError::MissingContract),
        );

        // Act + Assert (should panic)
        let result = contract_buy_icecream(&ctx, &mut host, ICECREAM_PRICE);
        claim_eq!(result, Err(ContractError::ContractError));
    }
}

#[concordium_cfg_test]
mod chain_tests {

    use super::*;
    use std::path::PathBuf;

    const INVOKER_ADDR: AccountAddress = AccountAddress([0; 32]);
    const WEATHER_SERVICE: ContractAddress = ContractAddress {
        index:    1,
        subindex: 0,
    };
    const ICECREAM_VENDOR: AccountAddress = AccountAddress([1; 32]);
    const ICECREAM_PRICE: Amount = Amount {
        micro_ccd: 6000000, // 6 CCD
    };

    #[derive(Debug)]
    struct FailedContractInteraction {
        energy: Energy,
        error:  ContractErrorType,
    }

    #[derive(Debug)]
    enum ContractErrorType {
        Binary(Vec<u8>),
        Typed, // TODO
    }

    struct Event(String);

    enum HostEvent {
        Interrupted(ContractAddress),
        Resumed(ContractAddress),
    }

    struct SuccessfulContractUpdate {
        events:                 Vec<Event>,
        interrupts_and_resumes: Vec<HostEvent>,
        transfers:              Vec<(AccountAddress, Amount)>,
        energy:                 Energy,
    }

    struct SuccessfulContractInit {
        contract_address: ContractAddress,
        events:           Vec<Event>,
        energy:           Energy,
    }

    struct Policies;

    struct Chain {
        slot_time: Option<SlotTime>,
    }

    #[derive(Debug)]
    struct Energy {
        energy: u64,
    }

    enum ContractParameter<P: Serialize> {
        Empty,
        Typed(P),
        Binary {
            parameter: PathBuf,
        },
        JSON {
            parameter: PathBuf,
            // Will try to use embedded schema if this is `None`.
            schema:    Option<PathBuf>,
        },
    }

    impl Chain {
        fn empty() -> Self {
            Self {
                slot_time: None,
            }
        }

        fn new(slot_time: SlotTime) -> Self {
            Self {
                slot_time: Some(slot_time),
            }
        }

        fn contract_init<P: Serialize>(
            &mut self,
            _sender: AccountAddress,
            _code: PathBuf,
            _contract_name: ContractName,
            _parameter: ContractParameter<P>,
            _amount: Amount,
        ) -> Result<SuccessfulContractInit, FailedContractInteraction> {
            todo!()
        }

        /// Should return
        fn contract_update<P: Serialize>(
            &mut self,
            _sender: AccountAddress,
            _address: ContractAddress,
            _entrypoint: EntrypointName,
            _parameter: ContractParameter<P>,
            _amount: Amount,
        ) -> Result<SuccessfulContractUpdate, FailedContractInteraction> {
            todo!()
        }

        fn contract_invoke<Rv: Deserial, P: Serialize>(
            &mut self,
            _sender: AccountAddress,
            _address: ContractAddress,
            _entrypoint: EntrypointName,
            _parameter: ContractParameter<P>,
            _amount: Amount,
        ) -> Result<SuccessfulContractUpdate, FailedContractInteraction> {
            todo!()
        }

        fn make_account_missing(&mut self, _account: AccountAddress) { todo!() }

        fn create_account(
            &mut self,
            _account: AccountAddress,
            _balance: Amount,
            _policies: Option<Policies>,
        ) {
            todo!()
        }

        /// Creates a contract address with an index one above the highest
        /// currently used. Next call to `contract_init` will skip this
        /// address.
        fn create_contract_address(&mut self) -> ContractAddress { todo!() }

        fn set_slot_time(&mut self, slot_time: SlotTime) { self.slot_time = Some(slot_time); }
    }

    fn test_sunny_days() {
        let mut chain = Chain::empty();

        chain.create_account(INVOKER_ADDR, Amount::from_ccd(10000), None);
        chain.create_account(ICECREAM_VENDOR, Amount::from_ccd(10000), None);

        let addr_weather = chain
            .contract_init(
                INVOKER_ADDR,
                PathBuf::from("a.wasm.v1"),
                ContractName::new_unchecked("init_weather"),
                ContractParameter::Typed(Weather::Sunny),
                Amount::zero(), // Must be < invoker.balance (TODO + energy cost ?)
            )
            .expect("Initializing weahter contract failed")
            .contract_address;

        let addr_icecream = chain
            .contract_init(
                ICECREAM_VENDOR,
                PathBuf::from("a.wasm.v1"),
                ContractName::new_unchecked("init_icecream"),
                ContractParameter::Typed(addr_weather),
                Amount::zero(),
            )
            .expect("Initializing icecream contract failed")
            .contract_address;

        let res = chain
            .contract_update(
                INVOKER_ADDR,
                addr_icecream,
                EntrypointName::new_unchecked("buy_icecream"),
                ContractParameter::Typed(ICECREAM_VENDOR),
                ICECREAM_PRICE,
            )
            .expect("Buying icecream update failed");

        assert_eq!(res.transfers, [(ICECREAM_VENDOR, ICECREAM_PRICE)]);
    }

    fn test_missing_weather_service() {
        let mut chain = Chain::empty();

        chain.create_account(INVOKER_ADDR, Amount::from_ccd(10000), None);
        chain.create_account(ICECREAM_VENDOR, Amount::from_ccd(10000), None);

        let unused_contract_address = chain.create_contract_address();

        let addr_icecream = chain
            .contract_init(
                ICECREAM_VENDOR,
                PathBuf::from("a.wasm.v1"),
                ContractName::new_unchecked("init_icecream"),
                ContractParameter::Typed(unused_contract_address),
                Amount::zero(),
            )
            .expect("Initializing icecream contract failed")
            .contract_address;

        let res = chain.contract_update(
            INVOKER_ADDR,
            addr_icecream,
            EntrypointName::new_unchecked("buy_icecream"),
            ContractParameter::Typed(ICECREAM_VENDOR),
            ICECREAM_PRICE,
        );

        assert!(res.is_err()); // TODO check exact error
    }
}
