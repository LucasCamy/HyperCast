#pragma once
namespace livekit {
template<class Send,class Drain>
int SendAndDrainCodec(int again,Send send,Drain drain) {
    int result=send();
    if(result==again){result=drain();if(result<0)return result;result=send();}
    if(result<0)return result;
    return drain();
}
}
