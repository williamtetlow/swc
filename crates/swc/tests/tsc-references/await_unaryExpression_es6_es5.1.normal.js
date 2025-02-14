import * as swcHelpers from "@swc/helpers";
import regeneratorRuntime from "regenerator-runtime";
function bar() {
    return _bar.apply(this, arguments);
}
function _bar() {
    _bar = // @target: es6
    swcHelpers.asyncToGenerator(regeneratorRuntime.mark(function _callee() {
        return regeneratorRuntime.wrap(function _callee$(_ctx) {
            while(1)switch(_ctx.prev = _ctx.next){
                case 0:
                    _ctx.next = 2;
                    return 42;
                case 2:
                    !_ctx.sent;
                case 3:
                case "end":
                    return _ctx.stop();
            }
        }, _callee);
    }));
    return _bar.apply(this, arguments);
}
function bar1() {
    return _bar1.apply(this, arguments);
}
function _bar1() {
    _bar1 = swcHelpers.asyncToGenerator(regeneratorRuntime.mark(function _callee() {
        return regeneratorRuntime.wrap(function _callee$(_ctx) {
            while(1)switch(_ctx.prev = _ctx.next){
                case 0:
                    _ctx.next = 2;
                    return 42;
                case 2:
                    +_ctx.sent;
                case 3:
                case "end":
                    return _ctx.stop();
            }
        }, _callee);
    }));
    return _bar1.apply(this, arguments);
}
function bar3() {
    return _bar3.apply(this, arguments);
}
function _bar3() {
    _bar3 = swcHelpers.asyncToGenerator(regeneratorRuntime.mark(function _callee() {
        return regeneratorRuntime.wrap(function _callee$(_ctx) {
            while(1)switch(_ctx.prev = _ctx.next){
                case 0:
                    _ctx.next = 2;
                    return 42;
                case 2:
                    -_ctx.sent;
                case 3:
                case "end":
                    return _ctx.stop();
            }
        }, _callee);
    }));
    return _bar3.apply(this, arguments);
}
function bar4() {
    return _bar4.apply(this, arguments);
}
function _bar4() {
    _bar4 = swcHelpers.asyncToGenerator(regeneratorRuntime.mark(function _callee() {
        return regeneratorRuntime.wrap(function _callee$(_ctx) {
            while(1)switch(_ctx.prev = _ctx.next){
                case 0:
                    _ctx.next = 2;
                    return 42;
                case 2:
                    ~_ctx.sent;
                case 3:
                case "end":
                    return _ctx.stop();
            }
        }, _callee);
    }));
    return _bar4.apply(this, arguments);
}
