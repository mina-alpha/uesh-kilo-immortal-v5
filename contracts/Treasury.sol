// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/**
 * @title Treasury - UESH Kilo Immortal v5
 * @notice Minimal treasury contract for automated profit distribution.
 *         40% of all organism profits are auto-wired to OWNER via this contract.
 * @dev Deployed via CREATE2 for deterministic address across chains.
 *      Owner is set at deployment (msg.sender) and cannot be changed.
 */
contract Treasury {
    address public immutable owner;
    
    uint256 public totalWired;
    uint256 public totalReceived;
    
    event Deposit(address indexed from, uint256 amount, uint256 timestamp);
    event Wire(address indexed to, uint256 amount, uint256 timestamp);
    event EmergencyWithdraw(address indexed to, uint256 amount, uint256 timestamp);
    
    modifier onlyOwner() {
        require(msg.sender == owner, "Treasury: not owner");
        _;
    }
    
    constructor() {
        owner = msg.sender;
    }
    
    /**
     * @notice Receive ETH deposits from the organism
     */
    receive() external payable {
        totalReceived += msg.value;
        emit Deposit(msg.sender, msg.value, block.timestamp);
    }
    
    /**
     * @notice Wire profits to a destination address
     * @param to Recipient address (typically OWNER_METAMASK)
     * @param amount Amount in wei to wire
     */
    function wire(address payable to, uint256 amount) external onlyOwner {
        require(amount > 0, "Treasury: zero amount");
        require(address(this).balance >= amount, "Treasury: insufficient balance");
        
        totalWired += amount;
        
        (bool success, ) = to.call{value: amount}("");
        require(success, "Treasury: wire failed");
        
        emit Wire(to, amount, block.timestamp);
    }
    
    /**
     * @notice Emergency withdrawal - drain all funds to owner
     */
    function withdraw() external onlyOwner {
        uint256 bal = address(this).balance;
        require(bal > 0, "Treasury: empty");
        
        totalWired += bal;
        
        (bool success, ) = payable(owner).call{value: bal}("");
        require(success, "Treasury: withdraw failed");
        
        emit EmergencyWithdraw(owner, bal, block.timestamp);
    }
    
    /**
     * @notice Get contract balance
     * @return Current ETH balance in wei
     */
    function balance() external view returns (uint256) {
        return address(this).balance;
    }
    
    /**
     * @notice Get treasury stats
     * @return _totalReceived Total ETH ever received
     * @return _totalWired Total ETH wired to owner
     * @return _currentBalance Current balance
     */
    function stats() external view returns (
        uint256 _totalReceived,
        uint256 _totalWired,
        uint256 _currentBalance
    ) {
        return (totalReceived, totalWired, address(this).balance);
    }
}
