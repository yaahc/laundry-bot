cross build --target arm-unknown-linux-gnueabi &&
    ssh root@192.168.1.210 systemctl stop laundry-bot.service &&
    scp target/arm-unknown-linux-gnueabi/debug/laundry-bot root@192.168.1.210:. &&
    scp .env root@192.168.1.210:. &&
    ssh root@192.168.1.210 systemctl restart laundry-bot.service
