// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "forge-std/Script.sol";
import "../src/TestToken.sol";

contract Deploy is Script {
    function run() external {
        // Default: Anvil's first account key (local testing only)
        uint256 deployerPrivateKey = vm.envOr("PRIVATE_KEY", uint256(0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80));
        vm.startBroadcast(deployerPrivateKey);

        TestToken t = new TestToken();
        t.mint(vm.addr(deployerPrivateKey), 1000e18);

        vm.stopBroadcast();

        console.log("TestToken", address(t));
    }
}
