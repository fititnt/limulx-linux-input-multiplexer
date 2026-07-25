# tests (some internal notes)

> Setup linux desktop to 
> SPI_SETKEYBOARDDELAY: 0 (250s)
> SPI_SETKEYBOARDSPEED; 0 (2.5 r/s)


```bash
# main keyboard, w pressed by 5s
sudo evtest /dev/input/event10 | tee mainkeyboard_w_5s.log


# footswich, i pressed by 5s
sudo evtest /dev/input/event12 | tee footswich_i_5s.log


sudo ./limulx-linux-input-multiplexer-amd64 -v --initial-delay 200 --rapid-fire-delay -1 -d /dev/input/event5 -d /dev/input/event12 -d /dev/input/event10

# (limulx) main keyboard, w pressed by 5s
sudo evtest /dev/input/event257 | tee limulx__mainkeyboard_w_5s.log


# main keyboard, w pressed by 5s & footswich, i pressed by 5s
#sudo evtest /dev/input/event12 | tee mainkeyboard_w_5s+footswich_i_5s.log

rm mainkeyboard_w_5s.log
rm limulx__mainkeyboard_w_5s.log
```