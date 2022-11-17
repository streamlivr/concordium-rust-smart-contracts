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

#[derive(Serialize, SchemaType, Clone, Copy, Debug, PartialEq, Eq)]
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
    use std::path::{Path, PathBuf};

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
        /// Energy spent.
        energy: Energy,
        /// Error returned.
        error:  AContractError,
        /// Events emitted before the interaction failed. Events from failed
        /// updates are not stored on the chain, but can be useful for
        /// debugging.
        events: Vec<Event>,
    }

    #[derive(Debug)]
    struct AContractError(Vec<u8>);

    #[derive(Debug, PartialEq, Eq)]
    enum DeployModuleError {
        FileNotFound,
        InvalidModule,
        InsufficientFunds,
    }

    impl AContractError {
        fn deserial<T: Deserial>(&self) -> Result<T, ParsingError> { todo!() }

        fn deserial_to_json(&self, schema_file: &Path) -> Result<SerdeJSON, ParsingError> {
            todo!()
        }
    }

    #[derive(PartialEq, Eq, Debug)]
    struct Event(String);

    #[derive(PartialEq, Eq, Debug)]
    enum ChainEvent {
        Interrupted {
            address: ContractAddress,
            events:  Vec<Event>,
        },
        Resumed {
            address: ContractAddress,
            success: bool,
        },
        Upgraded {
            address: ContractAddress,
            from:    ModuleReference,
            to:      ModuleReference,
        },
    }

    struct SuccessfulContractUpdate {
        /// Host events that occured. This includes interrupts, resumes, and
        /// upgrades.
        host_events:  Vec<ChainEvent>,
        transfers:    Vec<(AccountAddress, Amount)>,
        /// Energy used.
        energy:       Energy,
        /// The returned value.
        return_value: ContractReturnValue,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct SuccessfulModuleDeployment {
        module_reference: ModuleReference,
        energy:           Energy,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct SuccessfulContractInit {
        /// The address of the new instance.
        contract_address: ContractAddress,
        /// Events produced during initialization.
        events:           Vec<Event>,
        /// Energy used.
        energy:           Energy,
    }

    struct Policies;

    struct Chain {
        /// The slot time viewable inside the smart contracts.
        /// An error is thrown if this is `None` and the contract tries to
        /// access it.
        slot_time: Option<SlotTime>,
    }

    // TODO: Consider creating an enum with Unlimited / Limit(Energy).
    #[derive(Debug, PartialEq, Eq)]
    struct Energy {
        energy: u64,
    }

    struct ContractReturnValue(Vec<u8>);

    #[derive(Debug, PartialEq, Eq)]
    enum ParsingError {
        /// Thrown by `deserial` on failure.
        ParsingFailed,
        /// Could not find schema file.
        MissingSchemaFile,
        /// The schema file could not be parsed.
        InvalidSchemaFile,
        /// The return value could not be parsed using the provided schema.
        ParsingToJSONFailed,
    }

    struct SerdeJSON;

    impl ContractReturnValue {
        fn deserial<T: Deserial>(&self) -> Result<T, ParsingError> { todo!() }

        // TODO: optional schema
        fn deserial_to_json(&self, schema_file: &Path) -> Result<SerdeJSON, ParsingError> {
            todo!()
        }
    }

    struct ContractParameter(Vec<u8>);

    enum ParameterError {
        MissingParameterFile,
        MissingSchemaFile,
        InvalidSchema,
        ParsingFailed,
    }

    // TODO: Reconsider the API for using schemas, as we need the contract and
    // entrypoint names for parsing.
    impl ContractParameter {
        fn empty() -> Self { Self(Vec::new()) }

        fn from_bytes(bytes: Vec<u8>) -> Self { Self(bytes) }

        // TODO: optional schema
        fn from_json(parameter_file: &Path, schema_file: &Path) -> Result<Self, ParameterError> {
            todo!()
        }

        // TODO: add version with serde json
        fn from_typed<T: Serial>(parameter: &T) -> Self { Self(to_bytes(parameter)) }
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

        fn module_deploy(
            &mut self,
            _sender: AccountAddress,
            _code: PathBuf,
        ) -> Result<SuccessfulModuleDeployment, DeployModuleError> {
            todo!()
        }

        fn contract_init(
            &mut self,
            _sender: AccountAddress,
            _module: ModuleReference,
            _contract_name: ContractName,
            _parameter: ContractParameter,
            _amount: Amount,
            _energy: Option<Energy>, // Defaults to 100000 if `None`.
        ) -> Result<SuccessfulContractInit, FailedContractInteraction> {
            todo!()
        }

        /// Can we get the return value here?
        fn contract_update(
            &mut self,
            _sender: AccountAddress,
            _address: ContractAddress,
            _entrypoint: EntrypointName,
            _parameter: ContractParameter,
            _amount: Amount,
            _energy: Option<Energy>, // Defaults to 100000 if `None`.
        ) -> Result<SuccessfulContractUpdate, FailedContractInteraction> {
            todo!()
        }

        /// If `None` is provided, address 0 will be used, which will have
        /// sufficient funds.
        fn contract_invoke(
            &mut self,
            _sender: Option<AccountAddress>,
            _address: ContractAddress,
            _entrypoint: EntrypointName,
            _parameter: ContractParameter,
            _amount: Amount,
            _energy: Option<Energy>, // Defaults to 100000 if `None`.
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

        // TODO: Alternative ways of making addresses:
        //
        // Builder pattern:
        // chain.create_account().with_address(INVOKE_ADDR).with_balance(Amount::
        // from_ccd(10000));
        //
        // With default:
        // chain.create_account(TestAccount {
        //     balance: Some(Amount::from_ccd(10000)),
        //     ..Default::default()
        // });

        let mod_ref = chain
            .module_deploy(ICECREAM_VENDOR, PathBuf::from("a.wasm.v1"))
            .expect("Deployment of valid module should succeed.")
            .module_reference;

        let addr_weather = chain
            .contract_init(
                ICECREAM_VENDOR,
                mod_ref,
                ContractName::new_unchecked("init_weather"),
                ContractParameter::from_typed(&Weather::Sunny),
                Amount::zero(), // Must be <= invoker.balance + energy cost
                None,
            )
            .expect("Initializing weahter contract failed")
            .contract_address;

        let addr_icecream = chain
            .contract_init(
                ICECREAM_VENDOR,
                mod_ref,
                ContractName::new_unchecked("init_icecream"),
                ContractParameter::from_typed(&addr_weather),
                Amount::zero(),
                None,
            )
            .expect("Initializing icecream contract failed")
            .contract_address;

        let res = chain
            .contract_update(
                INVOKER_ADDR,
                addr_icecream,
                EntrypointName::new_unchecked("buy_icecream"),
                ContractParameter::from_typed(&ICECREAM_VENDOR),
                ICECREAM_PRICE,
                None,
            )
            .expect("Buying icecream update failed");
        // TODO: schema needs to know contr and entrypoint, but that is available here.
        // Add another function or chained function for handling it.

        assert_eq!(res.transfers, [(ICECREAM_VENDOR, ICECREAM_PRICE)]);
        assert_eq!(res.host_events, [ChainEvent::Interrupted {
            address: addr_icecream,
            events:  Vec::new(),
        },])
    }

    fn test_weather_init_and_invoke() {
        let mut chain = Chain::empty();

        chain.create_account(ICECREAM_VENDOR, Amount::from_ccd(10000), None);

        let mod_ref = chain
            .module_deploy(ICECREAM_VENDOR, PathBuf::from("a.wasm.v1"))
            .expect("Deployment of valid module should succeed.")
            .module_reference;

        let addr = chain
            .contract_init(
                ICECREAM_VENDOR,
                mod_ref,
                ContractName::new_unchecked("init_weather"),
                ContractParameter::from_typed(&Weather::Sunny),
                Amount::zero(),
                None,
            )
            .expect("Initializing weather contract failed.")
            .contract_address;

        let res = chain
            .contract_invoke(
                None,
                addr,
                EntrypointName::new_unchecked("get"),
                ContractParameter::empty(),
                Amount::zero(),
                None,
            )
            .expect("Invoking get entrypoint failed");
        assert_eq!(res.return_value.deserial(), Ok(Weather::Sunny));
        assert!(res.host_events.is_empty());
    }

    fn test_missing_weather_service() {
        let mut chain = Chain::empty();

        chain.create_account(INVOKER_ADDR, Amount::from_ccd(10000), None);
        chain.create_account(ICECREAM_VENDOR, Amount::from_ccd(10000), None);

        let unused_contract_address = chain.create_contract_address();

        let mod_ref = chain
            .module_deploy(ICECREAM_VENDOR, PathBuf::from("a.wasm.v1"))
            .expect("Deployment of valid module should succeed.")
            .module_reference;

        let addr_icecream = chain
            .contract_init(
                ICECREAM_VENDOR,
                mod_ref,
                ContractName::new_unchecked("init_icecream"),
                ContractParameter::from_typed(&unused_contract_address),
                Amount::zero(),
                None,
            )
            .expect("Initializing icecream contract failed")
            .contract_address;

        let res = chain.contract_update(
            INVOKER_ADDR,
            addr_icecream,
            EntrypointName::new_unchecked("buy_icecream"),
            ContractParameter::from_typed(&ICECREAM_VENDOR),
            ICECREAM_PRICE,
            None,
        );

        match res {
            Ok(_) => fail!("Update returned Ok(), but it should have failed."),
            Err(e) => assert_eq!(e.error.deserial(), Ok(ContractError::ContractError)),
        }
    }
}
