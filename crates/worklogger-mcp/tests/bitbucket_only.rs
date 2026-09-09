#![cfg(feature = "bitbucket")]

use std::collections::{BTreeMap, BTreeSet};

use rmcp::ServiceExt;
use worklogger_mcp::{
    BitbucketConfiguration, Capability, McpConfiguration, ModuleConfiguration, ModuleId,
    WorkloggerMcpServer,
};

#[tokio::test]
async fn bitbucket_module_runs_without_a_jira_connection() {
    let configuration = configuration();
    let server = WorkloggerMcpServer::new(configuration);
    let (server_transport, client_transport) = tokio::io::duplex(16_384);
    let server_task = tokio::spawn(async move {
        let service = server.serve(server_transport).await.expect("server starts");
        service.waiting().await.expect("server stops");
    });
    let client = ().serve(client_transport).await.expect("handshake");

    let tools = client.list_all_tools().await.expect("tools");

    assert_eq!(tools.len(), 4);
    assert!(tools.iter().all(|tool| tool.name.starts_with("bitbucket_")));
    client.cancel().await.expect("client stops");
    server_task.await.expect("server task joins");
}

fn configuration() -> McpConfiguration {
    let bitbucket = BitbucketConfiguration {
        email: "person@example.com".to_owned(),
        workspaces: BTreeMap::from([(
            "workspace".to_owned(),
            BTreeSet::from(["repository".to_owned()]),
        )]),
        request_timeout_seconds: 30,
        page_size: 50,
        maximum_collection_items: 100,
    };
    let module = ModuleConfiguration {
        enabled: true,
        capabilities: BTreeSet::from([Capability::ReadBitbucketPullRequests]),
    };
    McpConfiguration::new_bitbucket(bitbucket, BTreeMap::from([(ModuleId::Bitbucket, module)]))
        .expect("configuration is valid")
}
