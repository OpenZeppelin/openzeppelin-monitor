// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "forge-std/Test.sol";
import "../src/TestToken.sol";

contract TestTokenTest is Test {
    event Transfer(address indexed from_, address indexed to, uint256 value);

    TestToken token;
    address deployer = address(this);
    address recipient = address(0x1234);

    function setUp() public {
        token = new TestToken();
        token.mint(deployer, 1000e18);
    }

    function testMint() public view {
        assertEq(token.balanceOf(deployer), 1000e18);
    }

    function testTransfer() public {
        token.transfer(recipient, 100e18);
        assertEq(token.balanceOf(recipient), 100e18);
        assertEq(token.balanceOf(deployer), 900e18);
    }

    function testTransferEmitsEvent() public {
        vm.expectEmit(true, true, false, true);
        emit Transfer(deployer, recipient, 100e18);
        token.transfer(recipient, 100e18);
    }
}
