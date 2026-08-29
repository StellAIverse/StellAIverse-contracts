# StellAIverse Oracle Integration & Price Feed Contract

A comprehensive oracle system for reliable real-time price feeds and external data on Soroban. This implementation includes multi-provider support, price aggregation, circuit breakers, rate limiting, and incentive mechanisms.

## Features Implemented

### 1. Multiple Oracle Provider Support
- Chainlink integration support
- Pyth network compatibility
- Band protocol integration
- Custom oracle providers
- Primary and fallback provider classification

### 2. Price Feed Caching & Fallback Mechanisms
- Automatic fallback provider activation on primary failures
- Price caching with configurable staleness thresholds
- Graceful degradation when primary providers go offline
- Failure threshold configuration for fallback triggering

### 3. Data Freshness Validation
- Configurable maximum staleness periods per feed
- Automatic stale price detection
- Timestamp-based freshness verification
- Block number validation for update sequencing

### 4. Circuit Breaker for Extreme Price Movements
- Configurable maximum price change thresholds (basis points)
- Automatic circuit breaker triggering on extreme volatility
- Cooldown period with automatic reset
- Manual admin override capabilities
- Comprehensive event logging for all triggers

### 5. Historical Price Tracking
- Efficient storage of historical prices (last 1000 entries)
- Timestamp and block number indexed history
- Pagination support for querying historical data
- Aggregation flags for processed price points

### 6. Custom Data Feed Capabilities
- Support for non-price custom data feeds
- Flexible data type system
- Independent provider authorization per feed
- Separate staleness configuration

### 7. Price Aggregation from Multiple Sources
- Median price aggregation
- Weighted average based on provider reputation
- Outlier detection and removal using IQR method
- Divergence validation between providers
- Minimum source requirement enforcement

### 8. Oracle Node Incentive System
- Performance-based reward distribution
- Reputation scoring for providers
- Penalty mechanism for misbehavior
- Stake slashing for malicious updates
- Withdrawable incentive balances

### 9. Rate Limiting for Price Queries
- Tiered subscription system (Free, Basic, Premium, Unlimited)
- Global rate limits for all users
- Guest access limits for unsubscribed users
- Dynamic quota management
- Query counting and consumption tracking

## Technical Specifications

### Security Features
- Tamper-resistant price updates with authentication
- Minimum provider requirements (2+ sources)
- Outlier filtering to prevent price manipulation
- Reputation-based weighting to prioritize reliable providers
- Circuit breaker to prevent extreme price exploitation

### Gas Efficiency
- Historical price storage with circular buffer (prevents storage bloat)
- Efficient aggregation algorithms
- Minimal storage writes per query
- Optimized storage key generation

### Integration
- Compatible with existing DeFi contracts in the StellAIverse ecosystem
- Standardized price feed interfaces
- Easy integration with lending, AMM, and marketplace contracts

## Core Contract Functions

### Admin Functions
- `initialize()` - Setup contract with admin and treasury
- `register_provider()` - Add new oracle provider
- `create_price_feed()` - Create new price tracking feed
- `create_custom_feed()` - Create custom data feed
- `reset_circuit_breaker()` - Manually reset circuit breaker
- `add_fallback_provider()` - Add fallback provider to feed

### Provider Functions
- `submit_price()` - Submit price update for a feed
- `submit_custom_data()` - Submit custom data
- `withdraw_incentives()` - Withdraw earned rewards

### User Query Functions
- `get_aggregated_price()` - Get latest aggregated price
- `get_historical_prices()` - Query historical price data
- `get_custom_data()` - Get latest custom data
- `is_data_fresh()` - Check if price data is fresh
- `get_circuit_breaker_state()` - Get circuit breaker status

## Acceptance Criteria Verification

✅ **Price feeds update correctly from oracles** - Implemented with provider authorization and timestamp tracking
✅ **Stale price detection working** - Max staleness configuration with automatic validation
✅ **Circuit breaker triggers on extreme moves** - Basis point threshold checks with automatic triggering
✅ **Multiple oracle sources aggregate properly** - Median and weighted aggregation with outlier filtering
✅ **Historical prices queryable with timestamps** - Complete history tracking with pagination
✅ **Fallback oracles used when primary fails** - Automatic fallback activation with failure counting
✅ **Custom feed creation available** - Custom data feed support with independent configuration
✅ **Oracle node operators incentivized** - Performance-based rewards with penalty system
✅ **Query rate limiting enforced** - Tiered subscriptions with rate limiting and consumption tracking