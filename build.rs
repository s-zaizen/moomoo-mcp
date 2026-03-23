use std::{env, path::PathBuf};

fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("failed to vend protoc");
    unsafe {
        env::set_var("PROTOC", protoc);
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing"));
    let descriptor_path = out_dir.join("moomoo_descriptor.bin");

    let protos = [
        "proto/Common.proto",
        "proto/InitConnect.proto",
        "proto/KeepAlive.proto",
        "proto/GetGlobalState.proto",
        "proto/Qot_Common.proto",
        "proto/Qot_Sub.proto",
        "proto/Qot_GetSubInfo.proto",
        "proto/Qot_GetBasicQot.proto",
        "proto/Qot_GetStaticInfo.proto",
        "proto/Qot_GetSecuritySnapshot.proto",
        "proto/Qot_RequestHistoryKL.proto",
        "proto/Qot_RequestTradeDate.proto",
        "proto/Trd_Common.proto",
        "proto/Trd_GetAccList.proto",
        "proto/Trd_UnlockTrade.proto",
        "proto/Trd_GetFunds.proto",
        "proto/Trd_GetMaxTrdQtys.proto",
        "proto/Trd_GetPositionList.proto",
        "proto/Trd_GetOrderFillList.proto",
        "proto/Trd_GetOrderList.proto",
        "proto/Trd_GetHistoryOrderList.proto",
        "proto/Trd_GetHistoryOrderFillList.proto",
        "proto/Trd_GetOrderFee.proto",
        "proto/Trd_PlaceOrder.proto",
        "proto/Trd_ModifyOrder.proto",
    ];

    let mut config = prost_build::Config::new();
    config.include_file("moomoo_proto.rs");
    config.file_descriptor_set_path(&descriptor_path);
    config
        .compile_protos(&protos, &["proto"])
        .expect("failed to compile moomoo protos");

    println!("cargo:rerun-if-changed=proto");
}
