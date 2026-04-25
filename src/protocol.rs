use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::checksum::{calc_crc16_ccitt_false, verify_bcc};

pub const SERVER_ID: &str = "8981000000000000000";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Op {
    SetInterval,
    SetMode,
    FwBegin,
    FwChunk,
    FwEnd,
    GetStatus,
    StartMeasure,
    StopMeasure,
    StartMeasureOp,
    StopMeasureOp,
    GetMeasureOp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandRequest {
    #[serde(rename = "ICCID")]
    pub device_id: String,
    pub cmd_id: u32,
    pub expires: String,
    pub flags: u8,
    pub op: Op,
    pub arg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    Ok,
    Ng,
    BadCrc,
    Busy,
    Expired,
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AckMessage {
    #[serde(rename = "ICCID")]
    pub iccid: String,
    pub cmd_id: u32,
    pub status: Status,
    pub res: String,
    pub raw: String,
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("device_id must be 19-20 digits")]
    InvalidDeviceId,
    #[error("expires must be 12-digit JST timestamp (YYYYMMDDHHmm)")]
    InvalidExpires,
    #[error("flags must be 1")]
    InvalidFlags,
    #[error("field contains invalid comma")]
    CommaNotAllowed,
    #[error("argument format is invalid for operation")]
    InvalidArg,
    #[error("ack frame must be ICCID,cmd_id,status,res,bcc")]
    InvalidAckFormat,
    #[error("cmd_id must be unsigned integer")]
    InvalidCmdId,
    #[error("ack bcc verification failed")]
    InvalidAckBcc,
}

pub fn build_command_csv(req: &CommandRequest) -> Result<String, ProtocolError> {
    validate_command(req)?;

    let payload = format!(
        "{SERVER_ID},{},{},{},{},{},",
        req.cmd_id,
        req.expires,
        req.flags,
        op_to_wire(&req.op),
        req.arg
    );

    let crc = calc_crc16_ccitt_false(&payload);
    Ok(format!("{payload}{crc}"))
}

pub fn parse_ack_csv(line: &str) -> Result<AckMessage, ProtocolError> {
    if !verify_bcc(line) {
        return Err(ProtocolError::InvalidAckBcc);
    }

    let mut parts: Vec<&str> = line.split(',').collect();
    if parts.len() != 5 {
        return Err(ProtocolError::InvalidAckFormat);
    }

    let _bcc = parts.pop();
    let iccid = parts[0];
    let cmd_id = parts[1]
        .parse::<u32>()
        .map_err(|_| ProtocolError::InvalidCmdId)?;
    let status = status_from_wire(parts[2]);
    let res = parts[3].to_string();

    if !is_valid_device_id(iccid) {
        return Err(ProtocolError::InvalidDeviceId);
    }

    Ok(AckMessage {
        iccid: iccid.to_string(),
        cmd_id,
        status,
        res,
        raw: line.to_string(),
    })
}

fn validate_command(req: &CommandRequest) -> Result<(), ProtocolError> {
    if !is_valid_device_id(&req.device_id) {
        return Err(ProtocolError::InvalidDeviceId);
    }
    if !is_valid_timestamp12(&req.expires) {
        return Err(ProtocolError::InvalidExpires);
    }
    if req.flags != 1 {
        return Err(ProtocolError::InvalidFlags);
    }
    if req.device_id.contains(',') || req.expires.contains(',') || req.arg.contains(',') {
        return Err(ProtocolError::CommaNotAllowed);
    }

    validate_arg(&req.op, &req.arg)
}

fn validate_arg(op: &Op, arg: &str) -> Result<(), ProtocolError> {
    let requires_non_empty = matches!(
        op,
        Op::SetInterval | Op::SetMode | Op::FwBegin | Op::FwChunk | Op::FwEnd
    );
    let requires_empty = matches!(
        op,
        Op::GetStatus
            | Op::StartMeasure
            | Op::StopMeasure
            | Op::StartMeasureOp
            | Op::StopMeasureOp
            | Op::GetMeasureOp
    );

    if requires_non_empty && arg.is_empty() {
        return Err(ProtocolError::InvalidArg);
    }
    if requires_empty && !arg.is_empty() {
        return Err(ProtocolError::InvalidArg);
    }

    Ok(())
}

fn is_valid_device_id(input: &str) -> bool {
    (19..=20).contains(&input.len()) && input.chars().all(|c| c.is_ascii_digit())
}

fn is_valid_timestamp12(input: &str) -> bool {
    input.len() == 12 && input.chars().all(|c| c.is_ascii_digit())
}

fn op_to_wire(op: &Op) -> &'static str {
    match op {
        Op::SetInterval => "SET_INTERVAL",
        Op::SetMode => "SET_MODE",
        Op::FwBegin => "FW_BEGIN",
        Op::FwChunk => "FW_CHUNK",
        Op::FwEnd => "FW_END",
        Op::GetStatus => "GET_STATUS",
        Op::StartMeasure => "START_MEASURE",
        Op::StopMeasure => "STOP_MEASURE",
        Op::StartMeasureOp => "START_MEASURE_OP",
        Op::StopMeasureOp => "STOP_MEASURE_OP",
        Op::GetMeasureOp => "GET_MEASURE_OP",
    }
}

fn status_from_wire(status: &str) -> Status {
    match status {
        "OK" => Status::Ok,
        "NG" => Status::Ng,
        "BADCRC" => Status::BadCrc,
        "BUSY" => Status::Busy,
        "EXPIRED" => Status::Expired,
        other => Status::Unknown(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn builds_set_interval_command() {
        let req = CommandRequest {
            device_id: "8981123456789012345".to_string(),
            cmd_id: 2001,
            expires: "202602281300".to_string(),
            flags: 1,
            op: Op::SetInterval,
            arg: "interval:u32=600".to_string(),
        };

        let csv = build_command_csv(&req).expect("must build");
        assert_eq!(
            csv,
            "8981000000000000000,2001,202602281300,1,SET_INTERVAL,interval:u32=600,776D"
        );
    }

    #[test]
    fn builds_get_status_command() {
        let req = CommandRequest {
            device_id: "8981123456789012345".to_string(),
            cmd_id: 2003,
            expires: "202602281300".to_string(),
            flags: 1,
            op: Op::GetStatus,
            arg: "".to_string(),
        };

        let csv = build_command_csv(&req).expect("must build");
        assert_eq!(
            csv,
            "8981000000000000000,2003,202602281300,1,GET_STATUS,,2CB7"
        );
    }

    #[test]
    fn parses_ack_message() {
        let ack = parse_ack_csv(
            "8981123456789012345,2003,OK,fw:str=v1.2.0;dip:u8=1/0/12;interval:u32=600;mode:u8=2,34",
        )
        .expect("ack parse");

        assert_eq!(ack.iccid, "8981123456789012345");
        assert_eq!(ack.cmd_id, 2003);
        assert_eq!(ack.status, Status::Ok);
        assert_eq!(
            ack.res,
            "fw:str=v1.2.0;dip:u8=1/0/12;interval:u32=600;mode:u8=2"
        );
    }
}
