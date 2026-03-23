use rmcp::{
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::ServerInfo,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    config::Config,
    opend::{MoomooClient, MoomooError, get_global_state},
    opend_cmd::{OpenDCommandClient, OperationReply},
    proto::{
        GET_GLOBAL_STATE_RESPONSE, QOT_GET_BASIC_QOT_RESPONSE, QOT_GET_SECURITY_SNAPSHOT_RESPONSE,
        QOT_GET_STATIC_INFO_RESPONSE, QOT_GET_SUB_INFO_RESPONSE, QOT_REQUEST_HISTORY_KL_RESPONSE,
        QOT_REQUEST_TRADE_DATE_RESPONSE, QOT_SUB_RESPONSE, TRD_GET_ACC_LIST_RESPONSE,
        TRD_GET_FUNDS_RESPONSE, TRD_GET_HISTORY_ORDER_FILL_LIST_RESPONSE,
        TRD_GET_HISTORY_ORDER_LIST_RESPONSE, TRD_GET_MAX_TRD_QTYS_RESPONSE,
        TRD_GET_ORDER_FEE_RESPONSE, TRD_GET_ORDER_FILL_LIST_RESPONSE, TRD_GET_ORDER_LIST_RESPONSE,
        TRD_GET_POSITION_LIST_RESPONSE, TRD_MODIFY_ORDER_RESPONSE, TRD_PLACE_ORDER_RESPONSE,
        TRD_UNLOCK_TRADE_RESPONSE, common, message_to_json, qot_common, qot_get_basic_qot,
        qot_get_security_snapshot, qot_get_static_info, qot_get_sub_info, qot_request_history_kl,
        qot_request_trade_date, qot_sub, trd_common, trd_get_acc_list, trd_get_funds,
        trd_get_history_order_fill_list, trd_get_history_order_list, trd_get_max_trd_qtys,
        trd_get_order_fee, trd_get_order_fill_list, trd_get_order_list, trd_get_position_list,
        trd_modify_order, trd_place_order, trd_unlock_trade,
    },
};

const PROTO_ID_QOT_SUB: u32 = 3001;
const PROTO_ID_QOT_GET_SUB_INFO: u32 = 3003;
const PROTO_ID_QOT_GET_BASIC_QOT: u32 = 3004;
const PROTO_ID_QOT_REQUEST_HISTORY_KL: u32 = 3103;
const PROTO_ID_QOT_GET_STATIC_INFO: u32 = 3202;
const PROTO_ID_QOT_GET_SECURITY_SNAPSHOT: u32 = 3203;
const PROTO_ID_QOT_REQUEST_TRADE_DATE: u32 = 3219;
const PROTO_ID_TRD_GET_ACC_LIST: u32 = 2001;
const PROTO_ID_TRD_UNLOCK_TRADE: u32 = 2005;
const PROTO_ID_TRD_GET_FUNDS: u32 = 2101;
const PROTO_ID_TRD_GET_MAX_TRD_QTYS: u32 = 2111;
const PROTO_ID_TRD_GET_POSITION_LIST: u32 = 2102;
const PROTO_ID_TRD_GET_ORDER_FILL_LIST: u32 = 2211;
const PROTO_ID_TRD_GET_ORDER_LIST: u32 = 2201;
const PROTO_ID_TRD_GET_HISTORY_ORDER_LIST: u32 = 2221;
const PROTO_ID_TRD_GET_HISTORY_ORDER_FILL_LIST: u32 = 2222;
const PROTO_ID_TRD_GET_ORDER_FEE: u32 = 2225;
const PROTO_ID_TRD_PLACE_ORDER: u32 = 2202;
const PROTO_ID_TRD_MODIFY_ORDER: u32 = 2205;

#[derive(Debug, Clone)]
pub struct MoomooServer {
    client: MoomooClient,
    command_client: OpenDCommandClient,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ToolOutput {
    pub data: Value,
}

impl MoomooServer {
    pub fn new(config: Config) -> Self {
        Self {
            client: MoomooClient::new(config.clone()),
            command_client: OpenDCommandClient::new(config),
            tool_router: Self::tool_router(),
        }
    }
}

fn json_output(data: Value) -> Json<ToolOutput> {
    Json(ToolOutput { data })
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MoomooServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default().with_instructions(
            "Connects to local moomoo OpenD over TCP. Quote tools require market-qualified codes like US.AAPL or HK.00700. Trading tools can place or modify real orders if OpenD is logged into a live account. Auth recovery tools follow OpenD Operation Command and require Telnet / remote command to be enabled in OpenD.",
        )
    }
}

#[tool_router]
impl MoomooServer {
    #[tool(
        description = "Get OpenD global state, login state, server version, and market session state."
    )]
    async fn get_global_state(&self) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let response = get_global_state(&self.client).await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(GET_GLOBAL_STATE_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Interpret OpenD login and re-authentication state, including whether phone or picture verification or relogin is currently required."
    )]
    async fn get_auth_status(&self) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        Ok(json_output(load_auth_status(&self.client).await?))
    }

    #[tool(
        description = "Ask OpenD to relogin the current account through Operation Command. If no password is provided, OpenD reuses the startup password."
    )]
    async fn relogin_opend(
        &self,
        Parameters(request): Parameters<ReloginRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let password_md5 = match (request.password_md5.as_deref(), request.password) {
            (Some(value), _) => Some(normalize_md5_hex("password_md5", value)?),
            (None, Some(value)) => Some(format!("{:x}", md5::compute(value))),
            (None, None) => None,
        };
        let reply = self.command_client.relogin(password_md5.as_deref()).await?;
        Ok(json_output(
            build_auth_command_result(&self.client, reply).await,
        ))
    }

    #[tool(
        description = "Ask OpenD to send a phone verification code for the current login challenge."
    )]
    async fn request_phone_verify_code(&self) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let reply = self.command_client.request_phone_verify_code().await?;
        Ok(json_output(
            build_auth_command_result(&self.client, reply).await,
        ))
    }

    #[tool(description = "Submit the phone verification code that OpenD requested during login.")]
    async fn submit_phone_verify_code(
        &self,
        Parameters(request): Parameters<VerificationCodeRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let code = require_non_empty_text("code", &request.code)?;
        let reply = self.command_client.submit_phone_verify_code(code).await?;
        Ok(json_output(
            build_auth_command_result(&self.client, reply).await,
        ))
    }

    #[tool(
        description = "Ask OpenD to generate or display the current picture verification challenge."
    )]
    async fn request_picture_verify_code(
        &self,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let reply = self.command_client.request_picture_verify_code().await?;
        Ok(json_output(
            build_auth_command_result(&self.client, reply).await,
        ))
    }

    #[tool(description = "Submit the picture verification code that OpenD requested during login.")]
    async fn submit_picture_verify_code(
        &self,
        Parameters(request): Parameters<VerificationCodeRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let code = require_non_empty_text("code", &request.code)?;
        let reply = self.command_client.submit_picture_verify_code(code).await?;
        Ok(json_output(
            build_auth_command_result(&self.client, reply).await,
        ))
    }

    #[tool(
        description = "Get static instrument metadata. If codes are provided they take priority; otherwise query by market and security type."
    )]
    async fn get_static_info(
        &self,
        Parameters(request): Parameters<StaticInfoRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let (market, sec_type, security_list) = build_static_info_request(&request)?;
        let response: qot_get_static_info::Response = self
            .client
            .query(
                PROTO_ID_QOT_GET_STATIC_INFO,
                &qot_get_static_info::Request {
                    c2s: qot_get_static_info::C2s {
                        market,
                        sec_type,
                        security_list,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(QOT_GET_STATIC_INFO_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Get the trading calendar for a market or specific security between begin_time and end_time."
    )]
    async fn get_trade_dates(
        &self,
        Parameters(request): Parameters<TradeDatesRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let begin_time = require_non_empty_text("begin_time", &request.begin_time)?.to_string();
        let end_time = require_non_empty_text("end_time", &request.end_time)?.to_string();
        let security = request.code.as_deref().map(build_security).transpose()?;
        let market = resolve_trade_date_market(request.market.as_deref(), request.code.as_deref())?;
        let response: qot_request_trade_date::Response = self
            .client
            .query(
                PROTO_ID_QOT_REQUEST_TRADE_DATE,
                &qot_request_trade_date::Request {
                    c2s: qot_request_trade_date::C2s {
                        market,
                        begin_time,
                        end_time,
                        security,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(QOT_REQUEST_TRADE_DATE_RESPONSE, &response)
                .map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Inspect current quote subscriptions and remaining OpenD quote quota. By default only the current connection is returned."
    )]
    async fn get_quote_subscriptions(
        &self,
        Parameters(request): Parameters<QuoteSubscriptionsRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let response: qot_get_sub_info::Response = self
            .client
            .query(
                PROTO_ID_QOT_GET_SUB_INFO,
                &qot_get_sub_info::Request {
                    c2s: qot_get_sub_info::C2s {
                        is_req_all_conn: request.all_connections,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(QOT_GET_SUB_INFO_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Subscribe quote data for market-qualified symbols. If sub_types is omitted, BASIC is used so get_basic_quote can be called afterward."
    )]
    async fn subscribe_quotes(
        &self,
        Parameters(request): Parameters<QuoteSubscriptionRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let response = self.set_quote_subscription(&request, true, false).await?;
        Ok(json_output(response))
    }

    #[tool(
        description = "Unsubscribe quote data for market-qualified symbols. If sub_types is omitted, BASIC is used."
    )]
    async fn unsubscribe_quotes(
        &self,
        Parameters(request): Parameters<QuoteSubscriptionRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let response = self.set_quote_subscription(&request, false, false).await?;
        Ok(json_output(response))
    }

    #[tool(
        description = "Fetch real-time basic quotes for one or more market-qualified symbols, such as US.AAPL or HK.00700. OpenD requires a BASIC subscription first."
    )]
    async fn get_basic_quote(
        &self,
        Parameters(request): Parameters<SymbolsRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        require_non_empty_symbols(&request.codes)?;
        let securities = build_securities(&request.codes)?;
        let response: qot_get_basic_qot::Response = self
            .client
            .query(
                PROTO_ID_QOT_GET_BASIC_QOT,
                &qot_get_basic_qot::Request {
                    c2s: qot_get_basic_qot::C2s {
                        security_list: securities,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(QOT_GET_BASIC_QOT_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Fetch detailed quote snapshots for one or more market-qualified symbols, including bid/ask, 52-week range, and instrument-specific fields."
    )]
    async fn get_security_snapshot(
        &self,
        Parameters(request): Parameters<SymbolsRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        require_non_empty_symbols(&request.codes)?;
        let securities = build_securities(&request.codes)?;
        let response: qot_get_security_snapshot::Response = self
            .client
            .query(
                PROTO_ID_QOT_GET_SECURITY_SNAPSHOT,
                &qot_get_security_snapshot::Request {
                    c2s: qot_get_security_snapshot::C2s {
                        security_list: securities,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(QOT_GET_SECURITY_SNAPSHOT_RESPONSE, &response)
                .map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Fetch historical candles for a market-qualified symbol. Time strings must match moomoo's expected format like 2026-03-20 09:30:00."
    )]
    async fn get_history_kl(
        &self,
        Parameters(request): Parameters<HistoryKlRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let security = build_security(&request.code)?;
        let response: qot_request_history_kl::Response = self
            .client
            .query(
                PROTO_ID_QOT_REQUEST_HISTORY_KL,
                &qot_request_history_kl::Request {
                    c2s: qot_request_history_kl::C2s {
                        rehab_type: parse_named_enum(
                            "rehab_type",
                            &request.rehab_type,
                            &rehab_type_map(),
                        )?,
                        kl_type: parse_named_enum("kl_type", &request.kl_type, &kl_type_map())?,
                        security,
                        begin_time: request.begin_time,
                        end_time: request.end_time,
                        max_ack_kl_num: request.max_count,
                        need_kl_fields_flag: None,
                        next_req_key: None,
                        extended_time: request.extended_time,
                        session: request
                            .session
                            .as_deref()
                            .map(|value| parse_named_enum("session", value, &session_map()))
                            .transpose()?,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(QOT_REQUEST_HISTORY_KL_RESPONSE, &response)
                .map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "List trading accounts available to the currently logged-in moomoo/OpenD session."
    )]
    async fn list_accounts(
        &self,
        Parameters(request): Parameters<ListAccountsRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let response: trd_get_acc_list::Response = self
            .client
            .query(
                PROTO_ID_TRD_GET_ACC_LIST,
                &trd_get_acc_list::Request {
                    c2s: trd_get_acc_list::C2s {
                        user_id: 0,
                        trd_category: request
                            .trd_category
                            .as_deref()
                            .map(|value| {
                                parse_named_enum("trd_category", value, &trd_category_map())
                            })
                            .transpose()?,
                        need_general_sec_account: request.need_general_sec_account,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_GET_ACC_LIST_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Unlock or lock trade operations. If unlock is true, provide either password or password_md5. password_md5 must be lowercase hex."
    )]
    async fn unlock_trade(
        &self,
        Parameters(request): Parameters<UnlockTradeRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        if request.unlock && request.password.is_none() && request.password_md5.is_none() {
            return Err(MoomooError::InvalidParam {
                message: "unlock_trade requires password or password_md5 when unlock=true"
                    .to_string(),
            }
            .into());
        }

        let password_md5 = match (request.password_md5, request.password) {
            (Some(value), _) => Some(value),
            (None, Some(value)) => Some(format!("{:x}", md5::compute(value))),
            (None, None) => None,
        };

        let command = self.client.prepare_command().await?;
        let response: trd_unlock_trade::Response = self
            .client
            .execute_command(
                &command,
                PROTO_ID_TRD_UNLOCK_TRADE,
                &trd_unlock_trade::Request {
                    c2s: trd_unlock_trade::C2s {
                        unlock: request.unlock,
                        pwd_md5: password_md5,
                        security_firm: request
                            .security_firm
                            .as_deref()
                            .map(|value| {
                                parse_named_enum("security_firm", value, &security_firm_map())
                            })
                            .transpose()?,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_UNLOCK_TRADE_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    #[tool(description = "Fetch funds and buying power for a specific account and market.")]
    async fn get_funds(
        &self,
        Parameters(request): Parameters<AccountRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let response: trd_get_funds::Response = self
            .client
            .query(
                PROTO_ID_TRD_GET_FUNDS,
                &trd_get_funds::Request {
                    c2s: trd_get_funds::C2s {
                        header: build_trd_header(
                            request.acc_id,
                            &request.trd_env,
                            &request.trd_market,
                        )?,
                        refresh_cache: request.refresh_cache,
                        currency: request
                            .currency
                            .as_deref()
                            .map(|value| parse_named_enum("currency", value, &currency_map()))
                            .transpose()?,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_GET_FUNDS_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    #[tool(description = "Fetch current positions for a specific account and market.")]
    async fn get_positions(
        &self,
        Parameters(request): Parameters<PositionsRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let filter_conditions = build_filter_conditions(
            request.codes.as_deref(),
            None,
            None,
            None,
            request.filter_market.as_deref(),
        )?;
        let response: trd_get_position_list::Response = self
            .client
            .query(
                PROTO_ID_TRD_GET_POSITION_LIST,
                &trd_get_position_list::Request {
                    c2s: trd_get_position_list::C2s {
                        header: build_trd_header(
                            request.acc_id,
                            &request.trd_env,
                            &request.trd_market,
                        )?,
                        filter_conditions,
                        filter_pl_ratio_min: request.filter_pl_ratio_min,
                        filter_pl_ratio_max: request.filter_pl_ratio_max,
                        refresh_cache: request.refresh_cache,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_GET_POSITION_LIST_RESPONSE, &response)
                .map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Estimate the maximum tradable quantity for a potential order, using the same inputs OpenD expects for order validation."
    )]
    async fn get_max_trade_qtys(
        &self,
        Parameters(request): Parameters<MaxTradeQtysRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        validate_adjust_price_pair(request.adjust_price, request.adjust_side_and_limit)?;
        validate_order_identity(request.order_id, request.order_id_ex.as_deref())?;
        let (raw_code, derived_sec_market) = split_trade_code(&request.code)?;
        let sec_market = request
            .sec_market
            .as_deref()
            .map(|value| parse_named_enum("sec_market", value, &trd_sec_market_map()))
            .transpose()?
            .or(derived_sec_market);

        let response: trd_get_max_trd_qtys::Response = self
            .client
            .query(
                PROTO_ID_TRD_GET_MAX_TRD_QTYS,
                &trd_get_max_trd_qtys::Request {
                    c2s: trd_get_max_trd_qtys::C2s {
                        header: build_trd_header(
                            request.acc_id,
                            &request.trd_env,
                            &request.trd_market,
                        )?,
                        order_type: parse_named_enum(
                            "order_type",
                            &request.order_type,
                            &order_type_map(),
                        )?,
                        code: raw_code,
                        price: request.price,
                        order_id: request.order_id,
                        adjust_price: resolve_adjust_price_flag(
                            request.adjust_price,
                            request.adjust_side_and_limit,
                        ),
                        adjust_side_and_limit: request.adjust_side_and_limit,
                        sec_market,
                        order_id_ex: request.order_id_ex,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_GET_MAX_TRD_QTYS_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    #[tool(description = "Fetch open or filtered orders for a specific account and market.")]
    async fn get_orders(
        &self,
        Parameters(request): Parameters<OrdersRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let filter_conditions = build_filter_conditions(
            request.codes.as_deref(),
            request.begin_time.as_deref(),
            request.end_time.as_deref(),
            request.order_id_ex_list.as_deref(),
            request.filter_market.as_deref(),
        )?;
        let filter_status_list = request
            .filter_statuses
            .unwrap_or_default()
            .into_iter()
            .map(|value| parse_named_enum("filter_statuses", &value, &order_status_map()))
            .collect::<Result<Vec<_>, _>>()?;
        let response: trd_get_order_list::Response = self
            .client
            .query(
                PROTO_ID_TRD_GET_ORDER_LIST,
                &trd_get_order_list::Request {
                    c2s: trd_get_order_list::C2s {
                        header: build_trd_header(
                            request.acc_id,
                            &request.trd_env,
                            &request.trd_market,
                        )?,
                        filter_conditions,
                        filter_status_list,
                        refresh_cache: request.refresh_cache,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_GET_ORDER_LIST_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Fetch historical orders for a specific account and market. begin_time and end_time are required."
    )]
    async fn get_history_orders(
        &self,
        Parameters(request): Parameters<HistoryOrdersRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let filter_conditions = build_required_filter_conditions(
            request.codes.as_deref(),
            &request.begin_time,
            &request.end_time,
            request.order_id_ex_list.as_deref(),
            request.filter_market.as_deref(),
        )?;
        let filter_status_list = request
            .filter_statuses
            .unwrap_or_default()
            .into_iter()
            .map(|value| parse_named_enum("filter_statuses", &value, &order_status_map()))
            .collect::<Result<Vec<_>, _>>()?;
        let response: trd_get_history_order_list::Response = self
            .client
            .query(
                PROTO_ID_TRD_GET_HISTORY_ORDER_LIST,
                &trd_get_history_order_list::Request {
                    c2s: trd_get_history_order_list::C2s {
                        header: build_trd_header(
                            request.acc_id,
                            &request.trd_env,
                            &request.trd_market,
                        )?,
                        filter_conditions,
                        filter_status_list,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_GET_HISTORY_ORDER_LIST_RESPONSE, &response)
                .map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Fetch today's fills / executed trades for a specific account and market."
    )]
    async fn get_order_fills(
        &self,
        Parameters(request): Parameters<OrderFillsRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let filter_conditions = build_filter_conditions(
            request.codes.as_deref(),
            None,
            None,
            request.order_id_ex_list.as_deref(),
            request.filter_market.as_deref(),
        )?;
        let response: trd_get_order_fill_list::Response = self
            .client
            .query(
                PROTO_ID_TRD_GET_ORDER_FILL_LIST,
                &trd_get_order_fill_list::Request {
                    c2s: trd_get_order_fill_list::C2s {
                        header: build_trd_header(
                            request.acc_id,
                            &request.trd_env,
                            &request.trd_market,
                        )?,
                        filter_conditions,
                        refresh_cache: request.refresh_cache,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_GET_ORDER_FILL_LIST_RESPONSE, &response)
                .map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Fetch historical fills / executed trades for a specific account and market. begin_time and end_time are required."
    )]
    async fn get_history_order_fills(
        &self,
        Parameters(request): Parameters<HistoryOrderFillsRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let filter_conditions = build_required_filter_conditions(
            request.codes.as_deref(),
            &request.begin_time,
            &request.end_time,
            request.order_id_ex_list.as_deref(),
            request.filter_market.as_deref(),
        )?;
        let response: trd_get_history_order_fill_list::Response = self
            .client
            .query(
                PROTO_ID_TRD_GET_HISTORY_ORDER_FILL_LIST,
                &trd_get_history_order_fill_list::Request {
                    c2s: trd_get_history_order_fill_list::C2s {
                        header: build_trd_header(
                            request.acc_id,
                            &request.trd_env,
                            &request.trd_market,
                        )?,
                        filter_conditions,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_GET_HISTORY_ORDER_FILL_LIST_RESPONSE, &response)
                .map_err(MoomooError::from)?,
        )))
    }

    #[tool(description = "Fetch order-fee details for one or more server order IDs (order_id_ex).")]
    async fn get_order_fee(
        &self,
        Parameters(request): Parameters<OrderFeeRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        if request.order_id_ex_list.is_empty() {
            return Err(MoomooError::InvalidParam {
                message: "order_id_ex_list must not be empty".to_string(),
            }
            .into());
        }

        let response: trd_get_order_fee::Response = self
            .client
            .query(
                PROTO_ID_TRD_GET_ORDER_FEE,
                &trd_get_order_fee::Request {
                    c2s: trd_get_order_fee::C2s {
                        header: build_trd_header(
                            request.acc_id,
                            &request.trd_env,
                            &request.trd_market,
                        )?,
                        order_id_ex_list: request.order_id_ex_list,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_GET_ORDER_FEE_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    #[tool(
        description = "Place a trade order. This can create a real order in moomoo/OpenD if trd_env=REAL."
    )]
    async fn place_order(
        &self,
        Parameters(request): Parameters<PlaceOrderRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        let command = self.client.prepare_command().await?;
        let (raw_code, derived_sec_market) = split_trade_code(&request.code)?;
        let sec_market = request
            .sec_market
            .as_deref()
            .map(|value| parse_named_enum("sec_market", value, &trd_sec_market_map()))
            .transpose()?
            .or(derived_sec_market);

        let response: trd_place_order::Response = self
            .client
            .execute_command(
                &command,
                PROTO_ID_TRD_PLACE_ORDER,
                &trd_place_order::Request {
                    c2s: trd_place_order::C2s {
                        packet_id: common::PacketId {
                            conn_id: command.conn_id,
                            serial_no: command.serial,
                        },
                        header: build_trd_header(
                            request.acc_id,
                            &request.trd_env,
                            &request.trd_market,
                        )?,
                        trd_side: parse_named_enum("trd_side", &request.trd_side, &trd_side_map())?,
                        order_type: parse_named_enum(
                            "order_type",
                            &request.order_type,
                            &order_type_map(),
                        )?,
                        code: raw_code,
                        qty: request.qty,
                        price: request.price,
                        adjust_price: request.adjust_price,
                        adjust_side_and_limit: request.adjust_side_and_limit,
                        sec_market,
                        remark: request.remark,
                        time_in_force: request
                            .time_in_force
                            .as_deref()
                            .map(|value| {
                                parse_named_enum("time_in_force", value, &time_in_force_map())
                            })
                            .transpose()?,
                        fill_outside_rth: request.fill_outside_rth,
                        aux_price: request.aux_price,
                        trail_type: request
                            .trail_type
                            .as_deref()
                            .map(|value| parse_named_enum("trail_type", value, &trail_type_map()))
                            .transpose()?,
                        trail_value: request.trail_value,
                        trail_spread: request.trail_spread,
                        session: request
                            .session
                            .as_deref()
                            .map(|value| parse_named_enum("session", value, &session_map()))
                            .transpose()?,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_PLACE_ORDER_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    #[tool(description = "Modify, cancel, enable, disable, delete, or bulk-cancel orders.")]
    async fn modify_order(
        &self,
        Parameters(request): Parameters<ModifyOrderRequest>,
    ) -> Result<Json<ToolOutput>, rmcp::model::ErrorData> {
        if request.order_id.is_none()
            && request.order_id_ex.is_none()
            && !request.for_all.unwrap_or(false)
        {
            return Err(MoomooError::InvalidParam {
                message: "modify_order requires order_id or order_id_ex unless for_all=true"
                    .to_string(),
            }
            .into());
        }

        let command = self.client.prepare_command().await?;
        let response: trd_modify_order::Response = self
            .client
            .execute_command(
                &command,
                PROTO_ID_TRD_MODIFY_ORDER,
                &trd_modify_order::Request {
                    c2s: trd_modify_order::C2s {
                        packet_id: common::PacketId {
                            conn_id: command.conn_id,
                            serial_no: command.serial,
                        },
                        header: build_trd_header(
                            request.acc_id,
                            &request.trd_env,
                            &request.trd_market,
                        )?,
                        order_id: request.order_id.unwrap_or(0),
                        modify_order_op: parse_named_enum(
                            "modify_order_op",
                            &request.modify_order_op,
                            &modify_order_op_map(),
                        )?,
                        for_all: request.for_all,
                        trd_market: request
                            .target_market
                            .as_deref()
                            .map(|value| {
                                parse_named_enum("target_market", value, &trd_market_map())
                            })
                            .transpose()?,
                        qty: request.qty,
                        price: request.price,
                        adjust_price: request.adjust_price,
                        adjust_side_and_limit: request.adjust_side_and_limit,
                        aux_price: request.aux_price,
                        trail_type: request
                            .trail_type
                            .as_deref()
                            .map(|value| parse_named_enum("trail_type", value, &trail_type_map()))
                            .transpose()?,
                        trail_value: request.trail_value,
                        trail_spread: request.trail_spread,
                        order_id_ex: request.order_id_ex,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;
        Ok(json_output(extract_s2c(
            message_to_json(TRD_MODIFY_ORDER_RESPONSE, &response).map_err(MoomooError::from)?,
        )))
    }

    async fn set_quote_subscription(
        &self,
        request: &QuoteSubscriptionRequest,
        subscribe: bool,
        unsub_all: bool,
    ) -> Result<Value, rmcp::model::ErrorData> {
        if !unsub_all {
            require_non_empty_symbols(&request.codes)?;
        }

        let response: qot_sub::Response = self
            .client
            .query(
                PROTO_ID_QOT_SUB,
                &qot_sub::Request {
                    c2s: qot_sub::C2s {
                        security_list: if unsub_all {
                            Vec::new()
                        } else {
                            build_securities(&request.codes)?
                        },
                        sub_type_list: build_sub_types(request.sub_types.as_deref())?,
                        is_sub_or_un_sub: subscribe,
                        is_reg_or_un_reg_push: request.register_push,
                        reg_push_rehab_type_list: build_rehab_types(
                            request.rehab_types.as_deref(),
                        )?,
                        is_first_push: request.first_push,
                        is_unsub_all: Some(unsub_all),
                        is_sub_order_book_detail: request.order_book_detail,
                        extended_time: request.extended_time,
                        session: request
                            .session
                            .as_deref()
                            .map(|value| parse_named_enum("session", value, &session_map()))
                            .transpose()?,
                    },
                },
            )
            .await?;
        ensure_success(
            response.ret_type,
            response.ret_msg.as_deref(),
            response.err_code,
        )?;

        let mut payload =
            extract_s2c(message_to_json(QOT_SUB_RESPONSE, &response).map_err(MoomooError::from)?);
        if let Value::Object(object) = &mut payload {
            object.insert(
                "subscription".to_string(),
                json!({
                    "action": if subscribe { "subscribe" } else { "unsubscribe" },
                    "codes": request.codes,
                    "sub_types": request
                        .sub_types
                        .clone()
                        .unwrap_or_else(|| vec!["BASIC".to_string()]),
                }),
            );
        }
        Ok(payload)
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SymbolsRequest {
    /// Market-qualified symbols like US.AAPL or HK.00700.
    pub codes: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct QuoteSubscriptionsRequest {
    /// true to inspect every OpenD connection, false or omitted for only this MCP connection.
    pub all_connections: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct QuoteSubscriptionRequest {
    /// Market-qualified symbols like US.AAPL or HK.00700.
    pub codes: Vec<String>,
    /// Quote stream types such as BASIC, ORDER_BOOK, TICKER, RT, KL_DAY, KL_1MIN, KL_5MIN, KL_15MIN, KL_30MIN, KL_60MIN, KL_WEEK, KL_MONTH, KL_QUARTER, KL_YEAR, KL_3MIN, BROKER.
    pub sub_types: Option<Vec<String>>,
    /// Optional push registration toggle. Omit or false for request-response usage only.
    pub register_push: Option<bool>,
    /// Optional first-push toggle when registering push.
    pub first_push: Option<bool>,
    /// Optional rehab types for KL push subscriptions: NONE, FORWARD, BACKWARD.
    pub rehab_types: Option<Vec<String>>,
    /// Optional SF order-book detail toggle.
    pub order_book_detail: Option<bool>,
    /// Optional US extended-hours flag for RT/ticker/KL subscriptions.
    pub extended_time: Option<bool>,
    /// Optional US session: NONE, RTH, ETH, ALL, OVERNIGHT.
    pub session: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct StaticInfoRequest {
    /// Market-qualified symbols like US.AAPL or HK.00700. If present, market/sec_type are ignored.
    pub codes: Option<Vec<String>>,
    /// Optional market when querying a whole market/type bucket, like HK, US, SH, SZ, SG, JP, AU, MY, CA, FX.
    pub market: Option<String>,
    /// Optional security type when querying a whole market/type bucket. Defaults to STOCK.
    pub sec_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HistoryKlRequest {
    /// Market-qualified symbol like US.AAPL.
    pub code: String,
    /// K line type like 1MIN, 5MIN, 15MIN, 30MIN, 60MIN, DAY, WEEK, MONTH, QUARTER, YEAR.
    pub kl_type: String,
    /// Rehab type: NONE, FORWARD, BACKWARD.
    pub rehab_type: String,
    /// Begin time string like 2026-03-01 00:00:00.
    pub begin_time: String,
    /// End time string like 2026-03-22 23:59:59.
    pub end_time: String,
    /// Optional page size / maximum K-line count.
    pub max_count: Option<i32>,
    /// Optional US extended-hours toggle.
    pub extended_time: Option<bool>,
    /// Optional session for US data: NONE, RTH, ETH, ALL, OVERNIGHT.
    pub session: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TradeDatesRequest {
    /// Trade-date market such as HK, US, CN, NT, ST, JP_FUTURE, or SG_FUTURE. Optional if code implies HK/US/CN.
    pub market: Option<String>,
    /// Optional market-qualified security like US.AAPL. If provided, it narrows the calendar query.
    pub code: Option<String>,
    /// Begin date string like 2026-03-01.
    pub begin_time: String,
    /// End date string like 2026-03-31.
    pub end_time: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListAccountsRequest {
    /// Optional trade category: SECURITY or FUTURE.
    pub trd_category: Option<String>,
    /// SG-only flag for universal securities account lookup.
    pub need_general_sec_account: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct ReloginRequest {
    /// Plaintext moomoo login password. The server hashes this to lowercase MD5 before sending.
    pub password: Option<String>,
    /// Pre-hashed lowercase MD5 of the moomoo login password.
    pub password_md5: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct VerificationCodeRequest {
    pub code: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UnlockTradeRequest {
    /// true to unlock, false to lock.
    pub unlock: bool,
    /// Plaintext trade password. The server hashes this to lowercase MD5 before sending.
    pub password: Option<String>,
    /// Pre-hashed lowercase MD5 of the trade password.
    pub password_md5: Option<String>,
    /// Optional security firm: FUTU_SECURITIES, FUTU_INC, FUTU_SG, FUTU_AU, UNKNOWN.
    pub security_firm: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AccountRequest {
    pub acc_id: u64,
    /// REAL or SIMULATE.
    pub trd_env: String,
    /// HK, US, CN, HKCC, FUTURES, SG, AU, JP, MY, CA, HK_FUND, US_FUND, or one of the FUTURES_SIMULATE_* values.
    pub trd_market: String,
    /// Optional futures currency: HKD, USD, CNH, JPY, SGD, AUD, CAD, MYR.
    pub currency: Option<String>,
    pub refresh_cache: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MaxTradeQtysRequest {
    pub acc_id: u64,
    /// REAL or SIMULATE.
    pub trd_env: String,
    /// HK, US, CN, HKCC, FUTURES, SG, AU, JP, MY, CA, HK_FUND, US_FUND, or one of the FUTURES_SIMULATE_* values.
    pub trd_market: String,
    /// Market-qualified code like US.AAPL or HK.00700. Raw codes are accepted if sec_market is provided.
    pub code: String,
    /// NORMAL, MARKET, ABSOLUTE_LIMIT, AUCTION, AUCTION_LIMIT, SPECIAL_LIMIT, SPECIAL_LIMIT_ALL, STOP, STOP_LIMIT, MARKETIFTOUCHED, LIMITIFTOUCHED, TRAILING_STOP, TRAILING_STOP_LIMIT, TWAP_MARKET, TWAP_LIMIT, VWAP_MARKET, VWAP_LIMIT.
    pub order_type: String,
    /// For market/auction style orders, pass a current reference price so OpenD can estimate quantity.
    pub price: f64,
    /// Optional original numeric order ID when checking a modification scenario.
    pub order_id: Option<u64>,
    /// Optional server order ID when checking a modification scenario.
    pub order_id_ex: Option<String>,
    pub adjust_price: Option<bool>,
    pub adjust_side_and_limit: Option<f64>,
    pub sec_market: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PositionsRequest {
    pub acc_id: u64,
    pub trd_env: String,
    pub trd_market: String,
    /// Optional market-qualified symbol filters.
    pub codes: Option<Vec<String>>,
    pub filter_pl_ratio_min: Option<f64>,
    pub filter_pl_ratio_max: Option<f64>,
    /// Optional filter market override.
    pub filter_market: Option<String>,
    pub refresh_cache: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OrdersRequest {
    pub acc_id: u64,
    pub trd_env: String,
    pub trd_market: String,
    /// Optional market-qualified symbol filters.
    pub codes: Option<Vec<String>>,
    pub begin_time: Option<String>,
    pub end_time: Option<String>,
    pub order_id_ex_list: Option<Vec<String>>,
    /// Optional order status filters like SUBMITTED, FILLED_ALL, CANCELLED_ALL.
    pub filter_statuses: Option<Vec<String>>,
    pub filter_market: Option<String>,
    pub refresh_cache: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HistoryOrdersRequest {
    pub acc_id: u64,
    pub trd_env: String,
    pub trd_market: String,
    /// Optional market-qualified symbol filters.
    pub codes: Option<Vec<String>>,
    pub begin_time: String,
    pub end_time: String,
    pub order_id_ex_list: Option<Vec<String>>,
    /// Optional order status filters like SUBMITTED, FILLED_ALL, CANCELLED_ALL.
    pub filter_statuses: Option<Vec<String>>,
    pub filter_market: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OrderFillsRequest {
    pub acc_id: u64,
    pub trd_env: String,
    pub trd_market: String,
    /// Optional market-qualified symbol filters.
    pub codes: Option<Vec<String>>,
    pub order_id_ex_list: Option<Vec<String>>,
    pub filter_market: Option<String>,
    pub refresh_cache: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HistoryOrderFillsRequest {
    pub acc_id: u64,
    pub trd_env: String,
    pub trd_market: String,
    /// Optional market-qualified symbol filters.
    pub codes: Option<Vec<String>>,
    pub begin_time: String,
    pub end_time: String,
    pub order_id_ex_list: Option<Vec<String>>,
    pub filter_market: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OrderFeeRequest {
    pub acc_id: u64,
    pub trd_env: String,
    pub trd_market: String,
    /// Server-side order IDs returned by order queries as orderIDEx / order_id_ex.
    pub order_id_ex_list: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PlaceOrderRequest {
    pub acc_id: u64,
    pub trd_env: String,
    pub trd_market: String,
    /// Market-qualified code like US.AAPL or HK.00700. Raw codes are accepted if sec_market is provided.
    pub code: String,
    /// BUY, SELL, SELL_SHORT, BUY_BACK.
    pub trd_side: String,
    /// NORMAL, MARKET, ABSOLUTE_LIMIT, AUCTION, AUCTION_LIMIT, SPECIAL_LIMIT, SPECIAL_LIMIT_ALL, STOP, STOP_LIMIT, MARKETIFTOUCHED, LIMITIFTOUCHED, TRAILING_STOP, TRAILING_STOP_LIMIT, TWAP_MARKET, TWAP_LIMIT, VWAP_MARKET, VWAP_LIMIT.
    pub order_type: String,
    pub qty: f64,
    pub price: Option<f64>,
    pub sec_market: Option<String>,
    pub adjust_price: Option<bool>,
    pub adjust_side_and_limit: Option<f64>,
    pub remark: Option<String>,
    pub time_in_force: Option<String>,
    pub fill_outside_rth: Option<bool>,
    pub aux_price: Option<f64>,
    pub trail_type: Option<String>,
    pub trail_value: Option<f64>,
    pub trail_spread: Option<f64>,
    pub session: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ModifyOrderRequest {
    pub acc_id: u64,
    pub trd_env: String,
    pub trd_market: String,
    /// NORMAL, CANCEL, DISABLE, ENABLE, DELETE.
    pub modify_order_op: String,
    pub order_id: Option<u64>,
    pub order_id_ex: Option<String>,
    pub for_all: Option<bool>,
    /// Used when for_all=true.
    pub target_market: Option<String>,
    pub qty: Option<f64>,
    pub price: Option<f64>,
    pub adjust_price: Option<bool>,
    pub adjust_side_and_limit: Option<f64>,
    pub aux_price: Option<f64>,
    pub trail_type: Option<String>,
    pub trail_value: Option<f64>,
    pub trail_spread: Option<f64>,
}

fn require_non_empty_symbols(symbols: &[String]) -> Result<(), MoomooError> {
    if symbols.is_empty() {
        return Err(MoomooError::InvalidParam {
            message: "codes must not be empty".to_string(),
        });
    }
    Ok(())
}

fn validate_order_identity(
    order_id: Option<u64>,
    order_id_ex: Option<&str>,
) -> Result<(), MoomooError> {
    if order_id.is_some() && order_id_ex.is_some() {
        return Err(MoomooError::InvalidParam {
            message: "order_id and order_id_ex are mutually exclusive".to_string(),
        });
    }
    Ok(())
}

fn validate_adjust_price_pair(
    adjust_price: Option<bool>,
    adjust_side_and_limit: Option<f64>,
) -> Result<(), MoomooError> {
    if adjust_price == Some(false) && adjust_side_and_limit.is_some() {
        return Err(MoomooError::InvalidParam {
            message: "adjust_side_and_limit cannot be set when adjust_price=false".to_string(),
        });
    }
    Ok(())
}

fn resolve_adjust_price_flag(
    adjust_price: Option<bool>,
    adjust_side_and_limit: Option<f64>,
) -> Option<bool> {
    adjust_price.or(adjust_side_and_limit.map(|_| true))
}

fn require_non_empty_text<'a>(name: &str, value: &'a str) -> Result<&'a str, MoomooError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MoomooError::InvalidParam {
            message: format!("{name} must not be empty"),
        });
    }
    Ok(trimmed)
}

fn normalize_md5_hex(name: &str, value: &str) -> Result<String, MoomooError> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 32 || !normalized.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(MoomooError::InvalidParam {
            message: format!("{name} must be a 32-character hexadecimal MD5 string"),
        });
    }
    Ok(normalized)
}

fn build_static_info_request(
    request: &StaticInfoRequest,
) -> Result<(Option<i32>, Option<i32>, Vec<qot_common::Security>), MoomooError> {
    if let Some(codes) = request.codes.as_deref() {
        require_non_empty_symbols(codes)?;
        return Ok((None, None, build_securities(codes)?));
    }

    let market = request
        .market
        .as_deref()
        .ok_or_else(|| MoomooError::InvalidParam {
            message: "get_static_info requires codes or market".to_string(),
        })
        .and_then(parse_qot_market_prefix)?;
    let sec_type = request.sec_type.as_deref().unwrap_or("STOCK");
    Ok((
        Some(market),
        Some(parse_named_enum(
            "sec_type",
            sec_type,
            &security_type_map(),
        )?),
        Vec::new(),
    ))
}

fn build_securities(codes: &[String]) -> Result<Vec<qot_common::Security>, MoomooError> {
    codes.iter().map(|code| build_security(code)).collect()
}

fn build_sub_types(sub_types: Option<&[String]>) -> Result<Vec<i32>, MoomooError> {
    let Some(values) = sub_types else {
        return Ok(vec![qot_common::SubType::Basic as i32]);
    };
    if values.is_empty() {
        return Ok(vec![qot_common::SubType::Basic as i32]);
    }
    values
        .iter()
        .map(|value| parse_named_enum("sub_types", value, &sub_type_map()))
        .collect()
}

fn build_rehab_types(rehab_types: Option<&[String]>) -> Result<Vec<i32>, MoomooError> {
    rehab_types
        .unwrap_or_default()
        .iter()
        .map(|value| parse_named_enum("rehab_types", value, &rehab_type_map()))
        .collect()
}

fn build_security(code: &str) -> Result<qot_common::Security, MoomooError> {
    let (market_prefix, raw_code) = split_market_code(code)?;
    Ok(qot_common::Security {
        market: parse_qot_market_prefix(market_prefix)?,
        code: raw_code.to_string(),
    })
}

fn build_trd_header(
    acc_id: u64,
    trd_env: &str,
    trd_market: &str,
) -> Result<trd_common::TrdHeader, MoomooError> {
    Ok(trd_common::TrdHeader {
        trd_env: parse_named_enum("trd_env", trd_env, &trd_env_map())?,
        acc_id,
        trd_market: parse_named_enum("trd_market", trd_market, &trd_market_map())?,
    })
}

fn build_filter_conditions(
    codes: Option<&[String]>,
    begin_time: Option<&str>,
    end_time: Option<&str>,
    order_id_ex_list: Option<&[String]>,
    filter_market: Option<&str>,
) -> Result<Option<trd_common::TrdFilterConditions>, MoomooError> {
    let code_list = codes
        .unwrap_or_default()
        .iter()
        .map(|code| split_trade_code(code).map(|(raw, _)| raw))
        .collect::<Result<Vec<_>, _>>()?;
    let order_id_ex_list = order_id_ex_list.unwrap_or_default().to_vec();
    let filter_market = filter_market
        .map(|value| parse_named_enum("filter_market", value, &trd_market_map()))
        .transpose()?;

    if code_list.is_empty()
        && begin_time.is_none()
        && end_time.is_none()
        && order_id_ex_list.is_empty()
        && filter_market.is_none()
    {
        return Ok(None);
    }

    Ok(Some(trd_common::TrdFilterConditions {
        code_list,
        id_list: Vec::new(),
        begin_time: begin_time.map(ToOwned::to_owned),
        end_time: end_time.map(ToOwned::to_owned),
        order_id_ex_list,
        filter_market,
    }))
}

fn build_required_filter_conditions(
    codes: Option<&[String]>,
    begin_time: &str,
    end_time: &str,
    order_id_ex_list: Option<&[String]>,
    filter_market: Option<&str>,
) -> Result<trd_common::TrdFilterConditions, MoomooError> {
    let begin_time = require_non_empty_text("begin_time", begin_time)?.to_string();
    let end_time = require_non_empty_text("end_time", end_time)?.to_string();
    let code_list = codes
        .unwrap_or_default()
        .iter()
        .map(|code| split_trade_code(code).map(|(raw, _)| raw))
        .collect::<Result<Vec<_>, _>>()?;
    let order_id_ex_list = order_id_ex_list.unwrap_or_default().to_vec();
    let filter_market = filter_market
        .map(|value| parse_named_enum("filter_market", value, &trd_market_map()))
        .transpose()?;

    Ok(trd_common::TrdFilterConditions {
        code_list,
        id_list: Vec::new(),
        begin_time: Some(begin_time),
        end_time: Some(end_time),
        order_id_ex_list,
        filter_market,
    })
}

fn ensure_success(
    ret_type: i32,
    ret_msg: Option<&str>,
    err_code: Option<i32>,
) -> Result<(), MoomooError> {
    if ret_type == 0 {
        return Ok(());
    }

    Err(MoomooError::Api {
        message: if ret_msg.unwrap_or_default().is_empty() {
            format!("moomoo API returned retType={ret_type}")
        } else {
            ret_msg.unwrap_or_default().to_string()
        },
        err_code,
    })
}

fn extract_s2c(response: Value) -> Value {
    let Value::Object(mut object) = response else {
        return response;
    };

    let ret_msg = object
        .remove("retMsg")
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .filter(|value| !value.trim().is_empty());
    let data = object.remove("s2c");

    match (data, ret_msg) {
        (Some(Value::Object(mut data)), Some(ret_msg)) => {
            data.insert("retMsg".to_string(), Value::String(ret_msg));
            Value::Object(data)
        }
        (Some(data), Some(ret_msg)) => json!({ "data": data, "retMsg": ret_msg }),
        (Some(data), None) => data,
        (None, Some(ret_msg)) => json!({ "retMsg": ret_msg }),
        (None, None) => Value::Object(Map::new()),
    }
}

async fn load_auth_status(client: &MoomooClient) -> Result<Value, MoomooError> {
    let response = get_global_state(client).await?;
    ensure_success(
        response.ret_type,
        response.ret_msg.as_deref(),
        response.err_code,
    )?;
    summarize_auth_status(&response)
}

async fn build_auth_command_result(client: &MoomooClient, reply: OperationReply) -> Value {
    let mut payload = Map::new();
    payload.insert("command".to_string(), Value::String(reply.command));
    payload.insert("output".to_string(), Value::String(reply.output));
    match load_auth_status(client).await {
        Ok(auth_status) => {
            payload.insert("auth_status".to_string(), auth_status);
        }
        Err(error) => {
            payload.insert(
                "auth_status_error".to_string(),
                Value::String(error.to_string()),
            );
        }
    }
    Value::Object(payload)
}

fn summarize_auth_status(
    response: &crate::proto::get_global_state::Response,
) -> Result<Value, MoomooError> {
    let state = response.s2c.as_ref().ok_or_else(|| MoomooError::Protocol {
        message: "GetGlobalState response missing s2c".to_string(),
    })?;
    let program_status = state.program_status.as_ref();
    let program_status_code = program_status.map(|status| status.r#type);
    let (program_status_name, program_status_summary, recommended_action, ready_for_api) =
        describe_program_status(program_status_code, state.qot_logined, state.trd_logined);
    let program_status_message = program_status
        .and_then(|status| status.str_ext_desc.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    Ok(json!({
        "program_status": {
            "code": program_status_code,
            "name": program_status_name,
            "summary": program_status_summary,
            "message": program_status_message,
        },
        "qot_logined": state.qot_logined,
        "trd_logined": state.trd_logined,
        "ready_for_api": ready_for_api,
        "recommended_action": recommended_action,
        "conn_id": state.conn_id,
        "server_version": {
            "ver": state.server_ver,
            "build_no": state.server_build_no,
        },
        "time": state.time,
        "local_time": state.local_time,
    }))
}

fn describe_program_status(
    code: Option<i32>,
    qot_logined: bool,
    trd_logined: bool,
) -> (&'static str, &'static str, Option<&'static str>, bool) {
    match code {
        Some(value) if value == common::ProgramStatusType::Loaded as i32 => (
            "LOADED",
            "OpenD has started loading and is not ready yet.",
            Some("wait and poll get_auth_status again"),
            false,
        ),
        Some(value) if value == common::ProgramStatusType::Loging as i32 => (
            "LOGGING_IN",
            "OpenD is in the middle of a login attempt.",
            Some("wait and poll get_auth_status again"),
            false,
        ),
        Some(value) if value == common::ProgramStatusType::NeedPicVerifyCode as i32 => (
            "NEED_PIC_VERIFY_CODE",
            "OpenD is waiting for a picture verification code.",
            Some(
                "call request_picture_verify_code, inspect the reply, then call submit_picture_verify_code",
            ),
            false,
        ),
        Some(value) if value == common::ProgramStatusType::NeedPhoneVerifyCode as i32 => (
            "NEED_PHONE_VERIFY_CODE",
            "OpenD is waiting for a phone verification code.",
            Some("call request_phone_verify_code, then call submit_phone_verify_code"),
            false,
        ),
        Some(value) if value == common::ProgramStatusType::LoginFailed as i32 => (
            "LOGIN_FAILED",
            "OpenD failed to log in with the current credentials or challenge state.",
            Some("call relogin_opend or fix the OpenD startup credentials"),
            false,
        ),
        Some(value) if value == common::ProgramStatusType::ForceUpdate as i32 => (
            "FORCE_UPDATE",
            "OpenD is too old and must be updated before it can continue.",
            Some("update OpenD to a supported version"),
            false,
        ),
        Some(value) if value == common::ProgramStatusType::NessaryDataPreparing as i32 => (
            "NECESSARY_DATA_PREPARING",
            "OpenD is preparing required account data.",
            Some("wait and poll get_auth_status again"),
            false,
        ),
        Some(value) if value == common::ProgramStatusType::NessaryDataMissing as i32 => (
            "NECESSARY_DATA_MISSING",
            "OpenD is missing required account data for login completion.",
            Some("complete login in the OpenD UI or restart OpenD with login_account/login_pwd"),
            false,
        ),
        Some(value) if value == common::ProgramStatusType::UnAgreeDisclaimer as i32 => (
            "UNAGREE_DISCLAIMER",
            "The account must accept a disclaimer before OpenD can continue.",
            Some("accept the disclaimer in OpenD UI"),
            false,
        ),
        Some(value) if value == common::ProgramStatusType::Ready as i32 => {
            ("READY", "OpenD is ready to serve API requests.", None, true)
        }
        Some(value) if value == common::ProgramStatusType::ForceLogout as i32 => (
            "FORCE_LOGOUT",
            "OpenD was forced to log out, often after a password change or device-lock challenge.",
            Some("call relogin_opend"),
            false,
        ),
        Some(value) if value == common::ProgramStatusType::DisclaimerPullFailed as i32 => (
            "DISCLAIMER_PULL_FAILED",
            "OpenD failed to fetch disclaimer data needed for login completion.",
            Some("retry later or complete the flow in OpenD UI"),
            false,
        ),
        Some(value) if value == common::ProgramStatusType::None as i32 => (
            "NONE",
            "OpenD did not report a specific program status.",
            None,
            qot_logined && trd_logined,
        ),
        Some(_) | None => (
            "UNKNOWN",
            "OpenD returned an unknown or missing program status.",
            Some("check OpenD logs or UI, then retry"),
            qot_logined && trd_logined,
        ),
    }
}

fn split_market_code(code: &str) -> Result<(&str, &str), MoomooError> {
    let (prefix, raw_code) = code
        .split_once('.')
        .ok_or_else(|| MoomooError::InvalidParam {
            message: format!("market-qualified code is required, got '{code}'"),
        })?;
    Ok((prefix.trim(), raw_code.trim()))
}

fn split_trade_code(code: &str) -> Result<(String, Option<i32>), MoomooError> {
    if let Some((prefix, raw_code)) = code.split_once('.') {
        return Ok((
            raw_code.trim().to_string(),
            Some(parse_trd_sec_market_prefix(prefix.trim())?),
        ));
    }

    Ok((code.trim().to_string(), None))
}

fn resolve_trade_date_market(market: Option<&str>, code: Option<&str>) -> Result<i32, MoomooError> {
    if let Some(value) = market {
        return parse_named_enum("market", value, &trade_date_market_map());
    }

    let Some(code) = code else {
        return Err(MoomooError::InvalidParam {
            message: "get_trade_dates requires market or code".to_string(),
        });
    };

    let (prefix, _) = split_market_code(code)?;
    match normalize_enum(prefix).as_str() {
        "HK" => Ok(qot_common::TradeDateMarket::Hk as i32),
        "US" => Ok(qot_common::TradeDateMarket::Us as i32),
        "SH" | "SZ" | "CNSH" | "CNSZ" | "CN_SH" | "CN_SZ" => {
            Ok(qot_common::TradeDateMarket::Cn as i32)
        }
        other => Err(MoomooError::InvalidParam {
            message: format!(
                "cannot derive trade-date market from code prefix '{other}', pass market explicitly"
            ),
        }),
    }
}

fn parse_qot_market_prefix(prefix: &str) -> Result<i32, MoomooError> {
    match normalize_enum(prefix).as_str() {
        "HK" => Ok(qot_common::QotMarket::HkSecurity as i32),
        "US" => Ok(qot_common::QotMarket::UsSecurity as i32),
        "SH" | "CNSH" | "CN_SH" => Ok(qot_common::QotMarket::CnshSecurity as i32),
        "SZ" | "CNSZ" | "CN_SZ" => Ok(qot_common::QotMarket::CnszSecurity as i32),
        "SG" => Ok(qot_common::QotMarket::SgSecurity as i32),
        "JP" => Ok(qot_common::QotMarket::JpSecurity as i32),
        "AU" => Ok(qot_common::QotMarket::AuSecurity as i32),
        "MY" => Ok(qot_common::QotMarket::MySecurity as i32),
        "CA" => Ok(qot_common::QotMarket::CaSecurity as i32),
        "FX" => Ok(qot_common::QotMarket::FxSecurity as i32),
        other => Err(MoomooError::InvalidParam {
            message: format!("unsupported market prefix '{other}'"),
        }),
    }
}

fn parse_trd_sec_market_prefix(prefix: &str) -> Result<i32, MoomooError> {
    match normalize_enum(prefix).as_str() {
        "HK" => Ok(trd_common::TrdSecMarket::Hk as i32),
        "US" => Ok(trd_common::TrdSecMarket::Us as i32),
        "SH" | "CNSH" | "CN_SH" => Ok(trd_common::TrdSecMarket::CnSh as i32),
        "SZ" | "CNSZ" | "CN_SZ" => Ok(trd_common::TrdSecMarket::CnSz as i32),
        "SG" => Ok(trd_common::TrdSecMarket::Sg as i32),
        "JP" => Ok(trd_common::TrdSecMarket::Jp as i32),
        "AU" => Ok(trd_common::TrdSecMarket::Au as i32),
        "MY" => Ok(trd_common::TrdSecMarket::My as i32),
        "CA" => Ok(trd_common::TrdSecMarket::Ca as i32),
        "FX" => Ok(trd_common::TrdSecMarket::Fx as i32),
        other => Err(MoomooError::InvalidParam {
            message: format!("unsupported trading market prefix '{other}'"),
        }),
    }
}

fn parse_named_enum(name: &str, value: &str, mapping: &[(&str, i32)]) -> Result<i32, MoomooError> {
    let normalized = normalize_enum(value);
    mapping
        .iter()
        .find(|(candidate, _)| *candidate == normalized)
        .map(|(_, number)| *number)
        .ok_or_else(|| MoomooError::InvalidParam {
            message: format!("invalid value for {name}: {value}"),
        })
}

fn normalize_enum(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn security_type_map() -> Vec<(&'static str, i32)> {
    vec![
        ("UNKNOWN", qot_common::SecurityType::Unknown as i32),
        ("BOND", qot_common::SecurityType::Bond as i32),
        ("BWRT", qot_common::SecurityType::Bwrt as i32),
        ("STOCK", qot_common::SecurityType::Eqty as i32),
        ("EQTY", qot_common::SecurityType::Eqty as i32),
        ("ETF", qot_common::SecurityType::Trust as i32),
        ("FUND", qot_common::SecurityType::Trust as i32),
        ("TRUST", qot_common::SecurityType::Trust as i32),
        ("WARRANT", qot_common::SecurityType::Warrant as i32),
        ("INDEX", qot_common::SecurityType::Index as i32),
        ("IDX", qot_common::SecurityType::Index as i32),
        ("PLATE", qot_common::SecurityType::Plate as i32),
        ("OPTION", qot_common::SecurityType::Drvt as i32),
        ("DRVT", qot_common::SecurityType::Drvt as i32),
        ("PLATESET", qot_common::SecurityType::PlateSet as i32),
        ("FUTURE", qot_common::SecurityType::Future as i32),
    ]
}

fn trade_date_market_map() -> Vec<(&'static str, i32)> {
    vec![
        ("HK", qot_common::TradeDateMarket::Hk as i32),
        ("US", qot_common::TradeDateMarket::Us as i32),
        ("CN", qot_common::TradeDateMarket::Cn as i32),
        ("NT", qot_common::TradeDateMarket::Nt as i32),
        ("ST", qot_common::TradeDateMarket::St as i32),
        ("JP_FUTURE", qot_common::TradeDateMarket::JpFuture as i32),
        ("SG_FUTURE", qot_common::TradeDateMarket::SgFuture as i32),
    ]
}

fn trd_env_map() -> Vec<(&'static str, i32)> {
    vec![
        ("SIMULATE", trd_common::TrdEnv::Simulate as i32),
        ("REAL", trd_common::TrdEnv::Real as i32),
    ]
}

fn trd_category_map() -> Vec<(&'static str, i32)> {
    vec![
        ("UNKNOWN", trd_common::TrdCategory::Unknown as i32),
        ("SECURITY", trd_common::TrdCategory::Security as i32),
        ("FUTURE", trd_common::TrdCategory::Future as i32),
    ]
}

fn trd_market_map() -> Vec<(&'static str, i32)> {
    vec![
        ("UNKNOWN", trd_common::TrdMarket::Unknown as i32),
        ("HK", trd_common::TrdMarket::Hk as i32),
        ("US", trd_common::TrdMarket::Us as i32),
        ("CN", trd_common::TrdMarket::Cn as i32),
        ("HKCC", trd_common::TrdMarket::Hkcc as i32),
        ("FUTURES", trd_common::TrdMarket::Futures as i32),
        ("SG", trd_common::TrdMarket::Sg as i32),
        ("AU", trd_common::TrdMarket::Au as i32),
        ("JP", trd_common::TrdMarket::Jp as i32),
        ("MY", trd_common::TrdMarket::My as i32),
        ("CA", trd_common::TrdMarket::Ca as i32),
        ("HK_FUND", trd_common::TrdMarket::HkFund as i32),
        ("US_FUND", trd_common::TrdMarket::UsFund as i32),
        (
            "FUTURES_SIMULATE_HK",
            trd_common::TrdMarket::FuturesSimulateHk as i32,
        ),
        (
            "FUTURES_SIMULATE_US",
            trd_common::TrdMarket::FuturesSimulateUs as i32,
        ),
        (
            "FUTURES_SIMULATE_SG",
            trd_common::TrdMarket::FuturesSimulateSg as i32,
        ),
        (
            "FUTURES_SIMULATE_JP",
            trd_common::TrdMarket::FuturesSimulateJp as i32,
        ),
    ]
}

fn trd_sec_market_map() -> Vec<(&'static str, i32)> {
    vec![
        ("HK", trd_common::TrdSecMarket::Hk as i32),
        ("US", trd_common::TrdSecMarket::Us as i32),
        ("CN_SH", trd_common::TrdSecMarket::CnSh as i32),
        ("CN_SZ", trd_common::TrdSecMarket::CnSz as i32),
        ("SG", trd_common::TrdSecMarket::Sg as i32),
        ("JP", trd_common::TrdSecMarket::Jp as i32),
        ("AU", trd_common::TrdSecMarket::Au as i32),
        ("MY", trd_common::TrdSecMarket::My as i32),
        ("CA", trd_common::TrdSecMarket::Ca as i32),
        ("FX", trd_common::TrdSecMarket::Fx as i32),
    ]
}

fn trd_side_map() -> Vec<(&'static str, i32)> {
    vec![
        ("BUY", trd_common::TrdSide::Buy as i32),
        ("SELL", trd_common::TrdSide::Sell as i32),
        ("SELL_SHORT", trd_common::TrdSide::SellShort as i32),
        ("BUY_BACK", trd_common::TrdSide::BuyBack as i32),
    ]
}

fn order_type_map() -> Vec<(&'static str, i32)> {
    vec![
        ("NORMAL", trd_common::OrderType::Normal as i32),
        ("MARKET", trd_common::OrderType::Market as i32),
        (
            "ABSOLUTE_LIMIT",
            trd_common::OrderType::AbsoluteLimit as i32,
        ),
        ("AUCTION", trd_common::OrderType::Auction as i32),
        ("AUCTION_LIMIT", trd_common::OrderType::AuctionLimit as i32),
        ("SPECIAL_LIMIT", trd_common::OrderType::SpecialLimit as i32),
        (
            "SPECIAL_LIMIT_ALL",
            trd_common::OrderType::SpecialLimitAll as i32,
        ),
        ("STOP", trd_common::OrderType::Stop as i32),
        ("STOP_LIMIT", trd_common::OrderType::StopLimit as i32),
        (
            "MARKETIFTOUCHED",
            trd_common::OrderType::MarketifTouched as i32,
        ),
        (
            "LIMITIFTOUCHED",
            trd_common::OrderType::LimitifTouched as i32,
        ),
        ("TRAILING_STOP", trd_common::OrderType::TrailingStop as i32),
        (
            "TRAILING_STOP_LIMIT",
            trd_common::OrderType::TrailingStopLimit as i32,
        ),
        ("TWAP_MARKET", trd_common::OrderType::TwapMarket as i32),
        ("TWAP_LIMIT", trd_common::OrderType::TwapLimit as i32),
        ("VWAP_MARKET", trd_common::OrderType::VwapMarket as i32),
        ("VWAP_LIMIT", trd_common::OrderType::VwapLimit as i32),
    ]
}

fn modify_order_op_map() -> Vec<(&'static str, i32)> {
    vec![
        ("NORMAL", trd_common::ModifyOrderOp::Normal as i32),
        ("CANCEL", trd_common::ModifyOrderOp::Cancel as i32),
        ("DISABLE", trd_common::ModifyOrderOp::Disable as i32),
        ("ENABLE", trd_common::ModifyOrderOp::Enable as i32),
        ("DELETE", trd_common::ModifyOrderOp::Delete as i32),
    ]
}

fn security_firm_map() -> Vec<(&'static str, i32)> {
    vec![
        ("UNKNOWN", trd_common::SecurityFirm::Unknown as i32),
        (
            "FUTU_SECURITIES",
            trd_common::SecurityFirm::FutuSecurities as i32,
        ),
        ("FUTU_INC", trd_common::SecurityFirm::FutuInc as i32),
        ("FUTU_SG", trd_common::SecurityFirm::FutuSg as i32),
        ("FUTU_AU", trd_common::SecurityFirm::FutuAu as i32),
    ]
}

fn time_in_force_map() -> Vec<(&'static str, i32)> {
    vec![
        ("DAY", trd_common::TimeInForce::Day as i32),
        ("GTC", trd_common::TimeInForce::Gtc as i32),
    ]
}

fn trail_type_map() -> Vec<(&'static str, i32)> {
    vec![
        ("RATIO", trd_common::TrailType::Ratio as i32),
        ("AMOUNT", trd_common::TrailType::Amount as i32),
    ]
}

fn currency_map() -> Vec<(&'static str, i32)> {
    vec![
        ("HKD", trd_common::Currency::Hkd as i32),
        ("USD", trd_common::Currency::Usd as i32),
        ("CNH", trd_common::Currency::Cnh as i32),
        ("JPY", trd_common::Currency::Jpy as i32),
        ("SGD", trd_common::Currency::Sgd as i32),
        ("AUD", trd_common::Currency::Aud as i32),
        ("CAD", trd_common::Currency::Cad as i32),
        ("MYR", trd_common::Currency::Myr as i32),
    ]
}

fn order_status_map() -> Vec<(&'static str, i32)> {
    vec![
        ("UNSUBMITTED", trd_common::OrderStatus::Unsubmitted as i32),
        (
            "WAITING_SUBMIT",
            trd_common::OrderStatus::WaitingSubmit as i32,
        ),
        ("SUBMITTING", trd_common::OrderStatus::Submitting as i32),
        (
            "SUBMIT_FAILED",
            trd_common::OrderStatus::SubmitFailed as i32,
        ),
        ("TIME_OUT", trd_common::OrderStatus::TimeOut as i32),
        ("SUBMITTED", trd_common::OrderStatus::Submitted as i32),
        ("FILLED_PART", trd_common::OrderStatus::FilledPart as i32),
        ("FILLED_ALL", trd_common::OrderStatus::FilledAll as i32),
        (
            "CANCELLING_PART",
            trd_common::OrderStatus::CancellingPart as i32,
        ),
        (
            "CANCELLING_ALL",
            trd_common::OrderStatus::CancellingAll as i32,
        ),
        (
            "CANCELLED_PART",
            trd_common::OrderStatus::CancelledPart as i32,
        ),
        (
            "CANCELLED_ALL",
            trd_common::OrderStatus::CancelledAll as i32,
        ),
        ("FAILED", trd_common::OrderStatus::Failed as i32),
        ("DISABLED", trd_common::OrderStatus::Disabled as i32),
        ("DELETED", trd_common::OrderStatus::Deleted as i32),
        (
            "FILL_CANCELLED",
            trd_common::OrderStatus::FillCancelled as i32,
        ),
    ]
}

fn session_map() -> Vec<(&'static str, i32)> {
    vec![
        ("NONE", common::Session::None as i32),
        ("RTH", common::Session::Rth as i32),
        ("ETH", common::Session::Eth as i32),
        ("ALL", common::Session::All as i32),
        ("OVERNIGHT", common::Session::Overnight as i32),
    ]
}

fn rehab_type_map() -> Vec<(&'static str, i32)> {
    vec![
        ("NONE", qot_common::RehabType::None as i32),
        ("FORWARD", qot_common::RehabType::Forward as i32),
        ("BACKWARD", qot_common::RehabType::Backward as i32),
    ]
}

fn sub_type_map() -> Vec<(&'static str, i32)> {
    vec![
        ("NONE", qot_common::SubType::None as i32),
        ("BASIC", qot_common::SubType::Basic as i32),
        ("ORDER_BOOK", qot_common::SubType::OrderBook as i32),
        ("ORDERBOOK", qot_common::SubType::OrderBook as i32),
        ("TICKER", qot_common::SubType::Ticker as i32),
        ("RT", qot_common::SubType::Rt as i32),
        ("KL_DAY", qot_common::SubType::KlDay as i32),
        ("KL_1MIN", qot_common::SubType::Kl1min as i32),
        ("KL_3MIN", qot_common::SubType::Kl3min as i32),
        ("KL_5MIN", qot_common::SubType::Kl5min as i32),
        ("KL_15MIN", qot_common::SubType::Kl15min as i32),
        ("KL_30MIN", qot_common::SubType::Kl30min as i32),
        ("KL_60MIN", qot_common::SubType::Kl60min as i32),
        ("KL_WEEK", qot_common::SubType::KlWeek as i32),
        ("KL_MONTH", qot_common::SubType::KlMonth as i32),
        ("KL_QUARTER", qot_common::SubType::KlQurater as i32),
        ("KL_QURATER", qot_common::SubType::KlQurater as i32),
        ("KL_YEAR", qot_common::SubType::KlYear as i32),
        ("BROKER", qot_common::SubType::Broker as i32),
    ]
}

fn kl_type_map() -> Vec<(&'static str, i32)> {
    vec![
        ("1MIN", qot_common::KlType::KlType1min as i32),
        ("3MIN", qot_common::KlType::KlType3min as i32),
        ("5MIN", qot_common::KlType::KlType5min as i32),
        ("15MIN", qot_common::KlType::KlType15min as i32),
        ("30MIN", qot_common::KlType::KlType30min as i32),
        ("60MIN", qot_common::KlType::KlType60min as i32),
        ("DAY", qot_common::KlType::Day as i32),
        ("WEEK", qot_common::KlType::Week as i32),
        ("MONTH", qot_common::KlType::Month as i32),
        ("QUARTER", qot_common::KlType::Quarter as i32),
        ("YEAR", qot_common::KlType::Year as i32),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_trade_date_market_from_us_code() {
        let market = resolve_trade_date_market(None, Some("US.AAPL")).expect("market");
        assert_eq!(market, qot_common::TradeDateMarket::Us as i32);
    }

    #[test]
    fn static_info_request_prefers_codes() {
        let request = StaticInfoRequest {
            codes: Some(vec!["US.AAPL".to_string()]),
            market: Some("HK".to_string()),
            sec_type: Some("ETF".to_string()),
        };
        let (market, sec_type, securities) =
            build_static_info_request(&request).expect("static info request");
        assert_eq!(market, None);
        assert_eq!(sec_type, None);
        assert_eq!(securities.len(), 1);
        assert_eq!(securities[0].code, "AAPL");
    }

    #[test]
    fn rejects_conflicting_order_identity() {
        let error = validate_order_identity(Some(1), Some("abc")).expect_err("should fail");
        assert!(
            error
                .to_string()
                .contains("order_id and order_id_ex are mutually exclusive")
        );
    }
}
