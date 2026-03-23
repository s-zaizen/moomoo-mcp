use once_cell::sync::Lazy;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage};
use serde_json::Value;

include!(concat!(env!("OUT_DIR"), "/moomoo_proto.rs"));

pub const INIT_CONNECT_RESPONSE: &str = "InitConnect.Response";
pub const GET_GLOBAL_STATE_RESPONSE: &str = "GetGlobalState.Response";
pub const KEEP_ALIVE_RESPONSE: &str = "KeepAlive.Response";
pub const QOT_SUB_RESPONSE: &str = "Qot_Sub.Response";
pub const QOT_GET_SUB_INFO_RESPONSE: &str = "Qot_GetSubInfo.Response";
pub const QOT_GET_BASIC_QOT_RESPONSE: &str = "Qot_GetBasicQot.Response";
pub const QOT_GET_STATIC_INFO_RESPONSE: &str = "Qot_GetStaticInfo.Response";
pub const QOT_GET_SECURITY_SNAPSHOT_RESPONSE: &str = "Qot_GetSecuritySnapshot.Response";
pub const QOT_REQUEST_HISTORY_KL_RESPONSE: &str = "Qot_RequestHistoryKL.Response";
pub const QOT_REQUEST_TRADE_DATE_RESPONSE: &str = "Qot_RequestTradeDate.Response";
pub const TRD_GET_ACC_LIST_RESPONSE: &str = "Trd_GetAccList.Response";
pub const TRD_UNLOCK_TRADE_RESPONSE: &str = "Trd_UnlockTrade.Response";
pub const TRD_GET_FUNDS_RESPONSE: &str = "Trd_GetFunds.Response";
pub const TRD_GET_MAX_TRD_QTYS_RESPONSE: &str = "Trd_GetMaxTrdQtys.Response";
pub const TRD_GET_POSITION_LIST_RESPONSE: &str = "Trd_GetPositionList.Response";
pub const TRD_GET_ORDER_FILL_LIST_RESPONSE: &str = "Trd_GetOrderFillList.Response";
pub const TRD_GET_ORDER_LIST_RESPONSE: &str = "Trd_GetOrderList.Response";
pub const TRD_GET_HISTORY_ORDER_LIST_RESPONSE: &str = "Trd_GetHistoryOrderList.Response";
pub const TRD_GET_HISTORY_ORDER_FILL_LIST_RESPONSE: &str = "Trd_GetHistoryOrderFillList.Response";
pub const TRD_GET_ORDER_FEE_RESPONSE: &str = "Trd_GetOrderFee.Response";
pub const TRD_PLACE_ORDER_RESPONSE: &str = "Trd_PlaceOrder.Response";
pub const TRD_MODIFY_ORDER_RESPONSE: &str = "Trd_ModifyOrder.Response";

static DESCRIPTOR_POOL: Lazy<DescriptorPool> = Lazy::new(|| {
    DescriptorPool::decode(
        include_bytes!(concat!(env!("OUT_DIR"), "/moomoo_descriptor.bin")).as_ref(),
    )
    .expect("failed to decode moomoo descriptor set")
});

pub fn message_to_json<T>(full_name: &str, message: &T) -> Result<Value, prost::DecodeError>
where
    T: Message,
{
    let descriptor = DESCRIPTOR_POOL
        .get_message_by_name(full_name)
        .unwrap_or_else(|| panic!("missing descriptor for {full_name}"));
    let mut dynamic = DynamicMessage::new(descriptor);
    dynamic.transcode_from(message)?;
    Ok(serde_json::to_value(dynamic).expect("dynamic message should serialize"))
}
