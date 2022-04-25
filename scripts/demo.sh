#!/bin/bash

ENV="sandbox"
RED="\033[0;31m"
NC='\033[0m'
NEAR_PRICE="113400"

# Amount of tokens that have to be issued (total supply: 1000 tokens)
TOTAL_SUPPLY=1000000000000000000000000000

if [ $ENV == "testnet" ]; then
  SANDBOX=""

  near dev-deploy --wasm-file target/wasm32-unknown-unknown/testnet/usn.wasm --force

  ID=$(cat neardev/dev-account)
  # ID=(cat neardev/dev-account)

  near call $ID new '{"owner_id": "'${ID}'"}'  --account-id $ID

else
    SANDBOX=" --networkId sandbox --nodeUrl http://0.0.0.0:3030 --keyPath /tmp/near-usn-sandbox/validator_key.json"
    ID=test.near

    near deploy --wasm-file target/wasm32-unknown-unknown/sandbox/usn.wasm \
                --initFunction new \
                --initArgs '{"owner_id": "'${ID}'"}' \
                --account-id $ID \
                --master-account $ID \
                --force \
                $SANDBOX
fi

echo -e "${NC}"
near create-account bob.$ID --masterAccount $ID --initialBalance 1 $SANDBOX
near call $ID storage_deposit '' --accountId bob.$ID --amount 0.00125 $SANDBOX

echo -e "\n${RED}BOB BUYS SOME TOKENS:${NC}"
near call $ID extend_guardians --accountId $ID --args '{"guardians": ["'bob.$ID'"]}' $SANDBOX
near call $ID buy '{"expected": {"multiplier": "'$NEAR_PRICE'", "decimals": 28, "slippage": "10000"}}' --accountId bob.$ID --amount 0.1 $SANDBOX --gas 200000000000000
near view $ID ft_balance_of --args '{"account_id": "'bob.$ID'"}' $SANDBOX

near create-account alice.$ID --masterAccount $ID --initialBalance 1 $SANDBOX

echo -e "\n${RED}ALICE BUYS SOME TOKENS WITH AUTO-REGISTRATION:${NC}"
near call $ID buy --accountId alice.$ID --amount 0.1 $SANDBOX --gas 200000000000000

echo -e "\n${RED}BOB BUYS SOME TOKENS WITH WRONG SLIPPAGE:${NC}"

BALANCE1=$(near state "'bob.$ID'" | sed -n "s/.*formattedAmount: '\([^\\]*\).*'/\1/p")
echo "'$BALANCE1'"

near call $ID buy '{"expected": {"multiplier": "'$NEAR_PRICE'", "decimals": 28, "slippage": "1"}}' --accountId bob.$ID --amount 0.1 $SANDBOX --gas 200000000000000

BALANCE2=$(near state "'bob.$ID'" | sed -n "s/.*formattedAmount: '\([^\\]*\).*'/\1/p")
echo "'$BALANCE2'"
if [ "'$BALANCE1'" != "'$BALANCE2'" ]; then
  echo "Balance updated"
fi

near view $ID ft_balance_of --args '{"account_id": "'bob.$ID'"}' $SANDBOX

echo -e "\n${RED}BOB SELLS SOME TOKENS:${NC}"

near view $ID ft_balance_of --args '{"account_id": "'bob.$ID'"}' $SANDBOX
near state "'bob.$ID'" | sed -n "s/.*formattedAmount: '\([^\\]*\).*'/\1/p"

near call $ID sell '{"amount": "1000003499999999999", "expected": {"multiplier": "'$NEAR_PRICE'", "decimals": 28, "slippage": "10000"}}' --accountId bob.$ID --depositYocto 1 $SANDBOX --gas 200000000000000

near view $ID ft_balance_of --args '{"account_id": "'bob.$ID'"}' $SANDBOX

echo -e "\n${RED}TOTAL SUPPLY:${NC}"
near view $ID ft_total_supply --args '{}' $SANDBOX

echo -e "\n${RED}TRANSFER:${NC}"
near call $ID ft_transfer --accountId bob.$ID --args '{"receiver_id": "'$ID'", "amount": "1"}' --amount 0.000000000000000000000001 $SANDBOX

echo -e "\n${RED}IS BOB IN THE BLACKLIST:${NC}"
near call $ID blacklist_status --accountId $ID --args '{"account_id": "'bob.$ID'"}' $SANDBOX

echo -e "\n${RED}BOB TRYING HIMSELF ADD TO THE BLACKLIST:${NC}"
near call $ID add_to_blacklist --accountId bob.$ID --args '{"account_id": "'bob.$ID'"}' $SANDBOX
near call $ID blacklist_status --accountId $ID --args '{"account_id": "'bob.$ID'"}' $SANDBOX

echo -e "\n${RED}TEST.NEAR TRYING ADD BOB TO THE BLACKLIST:${NC}"
near call $ID add_to_blacklist --accountId $ID --args '{"account_id": "'bob.$ID'"}' $SANDBOX
near call $ID blacklist_status --accountId $ID --args '{"account_id": "'bob.$ID'"}' $SANDBOX

echo -e "\n${RED}BURN BANNED BOB FUNDS:${NC}"
near call $ID destroy_black_funds --accountId $ID --args '{"account_id": "'bob.$ID'"}' $SANDBOX
near view $ID ft_balance_of --args '{"account_id": "'bob.$ID'"}' $SANDBOX

echo -e "\n${RED}UNBAN BOB:${NC}"
near call $ID remove_from_blacklist --accountId $ID --args '{"account_id": "'bob.$ID'"}' $SANDBOX
near call $ID blacklist_status --accountId $ID --args '{"account_id": "'bob.$ID'"}' $SANDBOX

echo -e "\n${RED}MAINTENANCE ON:${NC}"
near call $ID pause --accountId $ID --args '{}' $SANDBOX
near call $ID contract_status --accountId $ID --args '{}' $SANDBOX

echo -e "\n${RED}TRANSFER:${NC}"
near call $ID ft_transfer --accountId $ID --args '{"receiver_id": "'bob.$ID'", "amount": "1"}' --amount 0.000000000000000000000001 $SANDBOX

echo -e "\n${RED}MAINTENANCE OFF:${NC}"
near call $ID resume --accountId $ID --args '{}' $SANDBOX
near call $ID contract_status --accountId $ID --args '{}' $SANDBOX
