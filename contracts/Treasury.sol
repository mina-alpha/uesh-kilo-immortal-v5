// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/**
 * @title Treasury — L2 Arbitrage Engine v5
 * @notice Minimal treasury contract for profit tracking and withdrawal.
 */
contract Treasury {
    address public immutable owner;

    uint256 public totalReceived;
    uint256 public totalWithdrawn;

    event Deposit(address indexed from, uint256 amount, uint256 timestamp);
    event Withdrawal(address indexed to, uint256 amount, uint256 timestamp);

    modifier onlyOwner() {
        require(msg.sender == owner, "Treasury: not owner");
        _;
    }

    constructor() {
        owner = msg.sender;
    }

    receive() external payable {
        totalReceived += msg.value;
        emit Deposit(msg.sender, msg.value, block.timestamp);
    }

    function withdraw(address payable to, uint256 amount) external onlyOwner {
        require(amount > 0, "Treasury: zero amount");
        require(address(this).balance >= amount, "Treasury: insufficient balance");

        totalWithdrawn += amount;

        (bool success, ) = to.call{value: amount}("");
        require(success, "Treasury: transfer failed");

        emit Withdrawal(to, amount, block.timestamp);
    }

    function emergencyWithdraw() external onlyOwner {
        uint256 bal = address(this).balance;
        require(bal > 0, "Treasury: empty");

        totalWithdrawn += bal;

        (bool success, ) = payable(owner).call{value: bal}("");
        require(success, "Treasury: transfer failed");

        emit Withdrawal(owner, bal, block.timestamp);
    }

    function balance() external view returns (uint256) {
        return address(this).balance;
    }

    function stats() external view returns (
        uint256 _totalReceived,
        uint256 _totalWithdrawn,
        uint256 _currentBalance
    ) {
        return (totalReceived, totalWithdrawn, address(this).balance);
    }
}
