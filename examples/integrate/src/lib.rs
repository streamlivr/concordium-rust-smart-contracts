//! # A Concordium V1 smart contract
use concordium_std::*;
use core::fmt::Debug;

/// Your smart contract state.
#[derive(Serialize, SchemaType, Clone)]
pub struct State {
    counter: u32,
}

/// Your smart contract errors.
#[derive(Debug, PartialEq, Eq, Reject, Serial, SchemaType)]
enum Error {
    /// Failed parsing the parameter.
    #[from(ParseError)]
    ParseParamsError,
    TransferErrorAmountTooLarge,
    TransferErrorMissingAccount,
}

impl From<TransferError> for Error {
    fn from(e: TransferError) -> Self {
        match e {
            TransferError::AmountTooLarge => Self::TransferErrorAmountTooLarge,
            TransferError::MissingAccount => Self::TransferErrorMissingAccount,
        }
    }
}

/// Init function that creates a new smart contract.
#[init(contract = "integrate")]
fn init<S: HasStateApi>(
    _ctx: &impl HasInitContext,
    _state_builder: &mut StateBuilder<S>,
) -> InitResult<State> {
    Ok(State {
        counter: 0,
    })
}

/// Receive function. The input parameter is the boolean variable `throw_error`.
///  If `throw_error == true`, the receive function will throw a custom error.
///  If `throw_error == false`, the receive function executes successfully.
#[receive(
    contract = "integrate",
    name = "receive",
    parameter = "AccountAddress",
    return_value = "u32",
    error = "Error",
    mutable,
    payable
)]
fn receive<S: HasStateApi>(
    ctx: &impl HasReceiveContext,
    host: &mut impl HasHost<State, StateApiType = S>,
    amount: Amount,
) -> Result<u32, Error> {
    let acc = ctx.parameter_cursor().get()?;
    host.state_mut().counter += 1;
    host.invoke_transfer(&acc, amount)?;
    host.state_mut().counter += 1;
    Ok(host.state().counter)
}

/// View function that returns the content of the state.
#[receive(contract = "integrate", name = "view", return_value = "u32")]
fn view<S: HasStateApi>(
    _ctx: &impl HasReceiveContext,
    host: &impl HasHost<State, StateApiType = S>,
) -> ReceiveResult<u32> {
    Ok(host.state().counter)
}
