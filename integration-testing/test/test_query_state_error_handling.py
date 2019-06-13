from .cl_node.wait import wait_for_blocks_count_at_least
from .cl_node.casperlabsnode import get_contract_state
from .cl_node.errors import NonZeroExitCodeError


import pytest
import casper_client
import logging

"""
aakoshh@af-dp:~/projects/CasperLabs/docker$ ./client.sh node-0 propose
Response: Success! Block 9d38836598... created and added.
aakoshh@af-dp:~/projects/CasperLabs/docker$ ./client.sh node-0 query-state --block-hash '"9d"' --key '"a91208047c"' --path file.xxx --type hash
NOT_FOUND: Cannot find block matching hash "9d"

aakoshh@af-dp:~/projects/CasperLabs/docker$ ./client.sh node-0 query-state --block-hash 9d --key '"a91208047c"' --path file.xxx --type hash
INVALID_ARGUMENT: Key of type hash must have exactly 32 bytes, 5 =/= 32 provided.

aakoshh@af-dp:~/projects/CasperLabs/docker$ ./client.sh node-0 query-state --block-hash 9d --key 3030303030303030303030303030303030303030303030303030303030303030 --path file.xxx --type hash
INVALID_ARGUMENT: Value not found: " Hash([48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48])"
aakoshh@af-dp:~/projects/CasperLabs/docker$
"""

KEY = '30' * 32
assert KEY == "3030303030303030303030303030303030303030303030303030303030303030"


@pytest.fixture()
def node(one_node_network):
    return one_node_network.docker_nodes[0]

@pytest.fixture()
def client(node):
    return node.d_client

@pytest.fixture()
def block_hash(node):
    return node.deploy_and_propose()

block_hash_queries = [
    (dict(blockHash = "9d", key = "a91208047c", path = "file.xxx", keyType = "hash"),
     "NOT_FOUND: Cannot find block matching"),

    (dict(                  key = "a91208047c", path = "file.xxx", keyType = "hash"),
     "INVALID_ARGUMENT: Key of type hash must have exactly 32 bytes"),

    (dict(                  key = KEY,          path = "file.xxx", keyType = "hash"),
     "INVALID_ARGUMENT: Value not found"),
]

@pytest.mark.parametrize("query, expected", block_hash_queries)
def test_query_state_error(client, block_hash, query, expected):
    if not 'blockHash' in query:
        query['blockHash'] = block_hash

    with pytest.raises(NonZeroExitCodeError) as excinfo:
        response = client.queryState(**query)
    assert expected in excinfo.value.output

